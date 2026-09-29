//! LSP request handling on top of the per-file analyses.

use std::collections::HashMap;
use std::path::Path;

use lsp_server::{ErrorCode, Message, Notification, Request, Response};
use lsp_types::notification::Notification as _;
use lsp_types::*;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::analysis::{Analysis, Namespace, Severity, SymbolKind as Kind};
use crate::text::{LineIndex, Span};
use crate::{bison, docs, flex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Bison,
    Flex,
}

const BISON_EXTENSIONS: &[&str] = &["y", "yy", "ypp", "bison"];
const FLEX_EXTENSIONS: &[&str] = &["l", "ll", "lex", "flex"];

impl Lang {
    fn detect(language_id: &str, uri: &Url) -> Option<Lang> {
        match language_id.to_ascii_lowercase().as_str() {
            "bison" | "yacc" => return Some(Lang::Bison),
            "flex" | "lex" => return Some(Lang::Flex),
            _ => {}
        }
        let ext = uri.path().rsplit_once('.')?.1;
        if BISON_EXTENSIONS.contains(&ext) {
            Some(Lang::Bison)
        } else if FLEX_EXTENSIONS.contains(&ext) {
            Some(Lang::Flex)
        } else {
            None
        }
    }

    fn analyze(self, text: &str) -> Analysis {
        match self {
            Lang::Bison => bison::analyze(text),
            Lang::Flex => flex::analyze(text),
        }
    }
}

pub struct Document {
    pub lang: Lang,
    pub text: String,
    pub index: LineIndex,
    pub analysis: Analysis,
}

impl Document {
    pub fn new(lang: Lang, text: String) -> Self {
        Self {
            lang,
            index: LineIndex::new(&text),
            analysis: lang.analyze(&text),
            text,
        }
    }

    fn offset(&self, pos: Position) -> usize {
        self.index.offset(&self.text, pos)
    }

    fn range(&self, span: &Span) -> lsp_types::Range {
        self.index.range(&self.text, span)
    }
}

/// A Bison token found for an identifier in a Flex action.
struct ExternalToken {
    uri: Url,
    range: lsp_types::Range,
    hover: String,
}

#[derive(Default)]
pub struct State {
    pub docs: HashMap<Url, Document>,
}

pub fn capabilities() -> ServerCapabilities {
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        definition_provider: Some(OneOf::Left(true)),
        references_provider: Some(OneOf::Left(true)),
        document_highlight_provider: Some(OneOf::Left(true)),
        document_symbol_provider: Some(OneOf::Left(true)),
        rename_provider: Some(OneOf::Right(RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: Default::default(),
        })),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(["%", "{", "<", "("].map(String::from).to_vec()),
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn respond<P: DeserializeOwned, R: Serialize>(req: Request, f: impl FnOnce(P) -> R) -> Response {
    match serde_json::from_value::<P>(req.params) {
        Ok(params) => Response::new_ok(req.id, f(params)),
        Err(e) => Response::new_err(req.id, ErrorCode::InvalidParams as i32, e.to_string()),
    }
}

impl State {
    pub fn handle_request(&self, req: Request) -> Response {
        use lsp_types::request::*;
        match req.method.as_str() {
            HoverRequest::METHOD => respond(req, |p: HoverParams| {
                let p = p.text_document_position_params;
                self.hover(&p.text_document.uri, p.position)
            }),
            GotoDefinition::METHOD => respond(req, |p: GotoDefinitionParams| {
                let p = p.text_document_position_params;
                self.definition(&p.text_document.uri, p.position)
            }),
            References::METHOD => respond(req, |p: ReferenceParams| {
                let pos = p.text_document_position;
                self.references(
                    &pos.text_document.uri,
                    pos.position,
                    p.context.include_declaration,
                )
            }),
            DocumentHighlightRequest::METHOD => respond(req, |p: DocumentHighlightParams| {
                let p = p.text_document_position_params;
                self.highlights(&p.text_document.uri, p.position)
            }),
            DocumentSymbolRequest::METHOD => respond(req, |p: DocumentSymbolParams| {
                self.document_symbols(&p.text_document.uri)
            }),
            PrepareRenameRequest::METHOD => respond(req, |p: TextDocumentPositionParams| {
                self.prepare_rename(&p.text_document.uri, p.position)
            }),
            Rename::METHOD => {
                let id = req.id.clone();
                match serde_json::from_value::<RenameParams>(req.params) {
                    Ok(p) => {
                        let pos = p.text_document_position;
                        match self.rename(&pos.text_document.uri, pos.position, &p.new_name) {
                            Ok(edit) => Response::new_ok(id, edit),
                            Err(msg) => Response::new_err(id, ErrorCode::InvalidParams as i32, msg),
                        }
                    }
                    Err(e) => Response::new_err(id, ErrorCode::InvalidParams as i32, e.to_string()),
                }
            }
            Completion::METHOD => respond(req, |p: CompletionParams| {
                let pos = p.text_document_position;
                self.completion(&pos.text_document.uri, pos.position)
            }),
            _ => Response::new_err(
                req.id,
                ErrorCode::MethodNotFound as i32,
                format!("unsupported request: {}", req.method),
            ),
        }
    }

    /// Returns messages to send back (diagnostics).
    pub fn handle_notification(&mut self, n: Notification) -> Vec<Message> {
        use lsp_types::notification::*;
        match n.method.as_str() {
            DidOpenTextDocument::METHOD => {
                let Ok(p) = serde_json::from_value::<DidOpenTextDocumentParams>(n.params) else {
                    return vec![];
                };
                let doc = p.text_document;
                let Some(lang) = Lang::detect(&doc.language_id, &doc.uri) else {
                    return vec![];
                };
                self.docs
                    .insert(doc.uri.clone(), Document::new(lang, doc.text));
                self.publish(&doc.uri)
            }
            DidChangeTextDocument::METHOD => {
                let Ok(p) = serde_json::from_value::<DidChangeTextDocumentParams>(n.params) else {
                    return vec![];
                };
                let uri = p.text_document.uri;
                let (Some(doc), Some(change)) = (
                    self.docs.get_mut(&uri),
                    p.content_changes.into_iter().last(),
                ) else {
                    return vec![];
                };
                *doc = Document::new(doc.lang, change.text);
                self.publish(&uri)
            }
            DidCloseTextDocument::METHOD => {
                let Ok(p) = serde_json::from_value::<DidCloseTextDocumentParams>(n.params) else {
                    return vec![];
                };
                let uri = p.text_document.uri;
                self.docs.remove(&uri);
                vec![publish_message(uri, vec![])]
            }
            _ => vec![],
        }
    }

    fn publish(&self, uri: &Url) -> Vec<Message> {
        let Some(doc) = self.docs.get(uri) else {
            return vec![];
        };
        vec![publish_message(uri.clone(), diagnostics(doc))]
    }

    fn doc_at(&self, uri: &Url, pos: Position) -> Option<(&Document, usize)> {
        let doc = self.docs.get(uri)?;
        Some((doc, doc.offset(pos)))
    }

    pub fn hover(&self, uri: &Url, pos: Position) -> Option<Hover> {
        let (doc, offset) = self.doc_at(uri, pos)?;
        let a = &doc.analysis;
        let (text, span) = if let Some(occ) = a.occurrence_at(offset) {
            let text = match a.symbol(occ.ns, &occ.name) {
                Some(sym) => sym.hover.clone(),
                None => builtin_hover(occ.ns, &occ.name)?,
            };
            (text, occ.span.clone())
        } else if let Some((key, span)) = a.directive_at(offset) {
            (docs::for_key(key)?, span.clone())
        } else if let Some((name, span)) = self.flex_code_ident(doc, offset) {
            (self.external_token(uri, name)?.hover, span.clone())
        } else {
            return None;
        };
        Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: text,
            }),
            range: Some(doc.range(&span)),
        })
    }

    fn flex_code_ident<'a>(&self, doc: &'a Document, offset: usize) -> Option<&'a (String, Span)> {
        (doc.lang == Lang::Flex)
            .then(|| doc.analysis.code_ident_at(offset))
            .flatten()
    }

    pub fn definition(&self, uri: &Url, pos: Position) -> Option<GotoDefinitionResponse> {
        let (doc, offset) = self.doc_at(uri, pos)?;
        if let Some(occ) = doc.analysis.occurrence_at(offset) {
            let sym = doc.analysis.symbol(occ.ns, &occ.name)?;
            return Some(GotoDefinitionResponse::Scalar(Location::new(
                uri.clone(),
                doc.range(&sym.selection),
            )));
        }
        let (name, _) = self.flex_code_ident(doc, offset)?;
        let token = self.external_token(uri, name)?;
        Some(GotoDefinitionResponse::Scalar(Location::new(
            token.uri,
            token.range,
        )))
    }

    /// Looks up a Bison token for an identifier used in a Flex action: first
    /// in open grammar files, then in grammar files next to the scanner.
    fn external_token(&self, flex_uri: &Url, name: &str) -> Option<ExternalToken> {
        let find = |uri: &Url, doc: &Document| {
            let sym = doc.analysis.symbol(Namespace::Grammar, name)?;
            (sym.kind == Kind::Token).then(|| ExternalToken {
                uri: uri.clone(),
                range: doc.range(&sym.selection),
                hover: sym.hover.clone(),
            })
        };
        let siblings = self.sibling_grammars(flex_uri);
        self.open_grammars()
            .into_iter()
            .chain(siblings.iter().map(|(u, d)| (u, d)))
            .find_map(|(u, d)| find(u, d))
    }

    /// Open Bison documents, in a stable order.
    fn open_grammars(&self) -> Vec<(&Url, &Document)> {
        let mut open: Vec<_> = self
            .docs
            .iter()
            .filter(|(_, d)| d.lang == Lang::Bison)
            .collect();
        open.sort_by_key(|(u, _)| u.as_str());
        open
    }

    /// Bison files next to `flex_uri` on disk that are not open.
    fn sibling_grammars(&self, flex_uri: &Url) -> Vec<(Url, Document)> {
        let Some(dir) = flex_uri
            .to_file_path()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
        else {
            return vec![];
        };
        let mut paths: Vec<_> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| has_extension(p, BISON_EXTENSIONS))
            .collect();
        paths.sort();
        paths
            .into_iter()
            .filter_map(|p| {
                let uri = Url::from_file_path(&p).ok()?;
                if self.docs.contains_key(&uri) {
                    return None;
                }
                let text = std::fs::read_to_string(&p).ok()?;
                Some((uri, Document::new(Lang::Bison, text)))
            })
            .collect()
    }

    pub fn references(
        &self,
        uri: &Url,
        pos: Position,
        include_declaration: bool,
    ) -> Option<Vec<Location>> {
        let (doc, offset) = self.doc_at(uri, pos)?;
        let occ = doc.analysis.occurrence_at(offset)?;
        Some(
            doc.analysis
                .occurrences_of(occ.ns, &occ.name)
                .filter(|o| include_declaration || !o.is_def)
                .map(|o| Location::new(uri.clone(), doc.range(&o.span)))
                .collect(),
        )
    }

    pub fn highlights(&self, uri: &Url, pos: Position) -> Option<Vec<DocumentHighlight>> {
        let (doc, offset) = self.doc_at(uri, pos)?;
        let occ = doc.analysis.occurrence_at(offset)?;
        Some(
            doc.analysis
                .occurrences_of(occ.ns, &occ.name)
                .map(|o| DocumentHighlight {
                    range: doc.range(&o.span),
                    kind: Some(if o.is_def {
                        DocumentHighlightKind::WRITE
                    } else {
                        DocumentHighlightKind::READ
                    }),
                })
                .collect(),
        )
    }

    pub fn document_symbols(&self, uri: &Url) -> Option<DocumentSymbolResponse> {
        let doc = self.docs.get(uri)?;
        #[allow(deprecated)]
        let symbols = doc
            .analysis
            .symbols
            .iter()
            .map(|s| DocumentSymbol {
                name: s.name.clone(),
                detail: Some(
                    match s.kind {
                        Kind::Token => "token",
                        Kind::Nonterminal => "nonterminal",
                        Kind::Definition => "definition",
                        Kind::Condition => "start condition",
                    }
                    .into(),
                ),
                kind: match s.kind {
                    Kind::Token => SymbolKind::CONSTANT,
                    Kind::Nonterminal => SymbolKind::FUNCTION,
                    Kind::Definition => SymbolKind::VARIABLE,
                    Kind::Condition => SymbolKind::ENUM_MEMBER,
                },
                tags: None,
                deprecated: None,
                range: doc.range(&s.range),
                selection_range: doc.range(&s.selection),
                children: None,
            })
            .collect();
        Some(DocumentSymbolResponse::Nested(symbols))
    }

    pub fn prepare_rename(&self, uri: &Url, pos: Position) -> Option<PrepareRenameResponse> {
        let (doc, offset) = self.doc_at(uri, pos)?;
        let occ = doc.analysis.occurrence_at(offset)?;
        doc.analysis.symbol(occ.ns, &occ.name)?;
        occ.renamable
            .then(|| PrepareRenameResponse::Range(doc.range(&occ.span)))
    }

    /// Returns `Ok(None)` when there is nothing renamable at `pos`, and an
    /// error message when `new_name` is not a valid name.
    pub fn rename(
        &self,
        uri: &Url,
        pos: Position,
        new_name: &str,
    ) -> Result<Option<WorkspaceEdit>, String> {
        let Some((doc, offset)) = self.doc_at(uri, pos) else {
            return Ok(None);
        };
        let Some(occ) = doc.analysis.occurrence_at(offset) else {
            return Ok(None);
        };
        if doc.analysis.symbol(occ.ns, &occ.name).is_none() {
            return Ok(None);
        }
        if !valid_name(doc.lang, new_name) {
            return Err(format!("`{new_name}` is not a valid name"));
        }
        let edits = doc
            .analysis
            .occurrences_of(occ.ns, &occ.name)
            .filter(|o| o.renamable)
            .map(|o| TextEdit::new(doc.range(&o.span), new_name.to_string()))
            .collect();
        Ok(Some(WorkspaceEdit {
            changes: Some(HashMap::from([(uri.clone(), edits)])),
            ..Default::default()
        }))
    }

    pub fn completion(&self, uri: &Url, pos: Position) -> Option<CompletionResponse> {
        let (doc, offset) = self.doc_at(uri, pos)?;
        let line_start = doc.text[..offset].rfind('\n').map_or(0, |i| i + 1);
        let before = &doc.text[line_start..offset];
        let word_len = before
            .bytes()
            .rev()
            .take_while(|&c| bison::is_ident_char(c) && c != b'.')
            .count();
        let before_word = &before[..before.len() - word_len];
        let word_range = doc.range(&(offset - word_len..offset));
        let items = match doc.lang {
            Lang::Bison => bison_completions(doc, offset, before_word),
            Lang::Flex => self.flex_completions(uri, doc, offset, before_word),
        };
        let items = items
            .into_iter()
            .map(|mut item| {
                // A `%` typed before the word is part of what gets replaced.
                let range = if item.label.starts_with('%') && before_word.ends_with('%') {
                    let mut r = word_range;
                    r.start.character -= 1;
                    r
                } else {
                    word_range
                };
                item.text_edit = Some(CompletionTextEdit::Edit(TextEdit::new(
                    range,
                    item.label.clone(),
                )));
                item
            })
            .collect();
        Some(CompletionResponse::Array(items))
    }

    fn flex_completions(
        &self,
        uri: &Url,
        doc: &Document,
        offset: usize,
        before_word: &str,
    ) -> Vec<CompletionItem> {
        let a = &doc.analysis;
        let in_rules = a.rules_start.is_some_and(|s| offset >= s);
        let trimmed = before_word.trim_end();
        if !in_rules && before_word == "%" {
            return directive_items(docs::FLEX_DIRECTIVES);
        }
        if !in_rules && before_word.starts_with("%option") {
            let mut items = directive_items(docs::FLEX_OPTIONS);
            for no in ["noyywrap", "nounput", "noinput", "nodefault", "nounistd"] {
                items.push(item(
                    no,
                    CompletionItemKind::PROPERTY,
                    docs::flex_option(&no[2..]),
                ));
            }
            return items;
        }
        let conditions = || {
            let mut items = symbol_items(a, Kind::Condition);
            items.push(item(
                "INITIAL",
                CompletionItemKind::ENUM_MEMBER,
                Some("The default start condition"),
            ));
            items
        };
        if before_word.ends_with('{') && !a.in_code(offset) {
            return symbol_items(a, Kind::Definition);
        }
        let in_condition_list = before_word.trim_start().starts_with('<')
            && before_word.trim_start()[1..]
                .bytes()
                .all(|c| c == b',' || c.is_ascii_alphanumeric() || c == b'_' || c == b'-');
        let after_begin = ["BEGIN", "BEGIN(", "yy_push_state("].iter().any(|p| {
            trimmed.ends_with(p) && (trimmed.len() < before_word.len() || p.ends_with('('))
        });
        if in_rules && in_condition_list || after_begin {
            return conditions();
        }
        if a.in_code(offset) {
            return self.token_items(uri);
        }
        vec![]
    }

    /// Tokens from Bison files the scanner can return.
    fn token_items(&self, flex_uri: &Url) -> Vec<CompletionItem> {
        let siblings = self.sibling_grammars(flex_uri);
        let mut items: Vec<CompletionItem> = self
            .open_grammars()
            .into_iter()
            .map(|(_, d)| d)
            .chain(siblings.iter().map(|(_, d)| d))
            .flat_map(|d| symbol_items(&d.analysis, Kind::Token))
            .collect();
        items.sort_by(|a, b| a.label.cmp(&b.label));
        items.dedup_by(|a, b| a.label == b.label);
        items
    }
}

fn bison_completions(doc: &Document, offset: usize, before_word: &str) -> Vec<CompletionItem> {
    let a = &doc.analysis;
    if a.in_code(offset) {
        return vec![];
    }
    if before_word.ends_with('%') {
        return directive_items(docs::BISON_DIRECTIVES);
    }
    let mut items = symbol_items(a, Kind::Nonterminal);
    items.extend(symbol_items(a, Kind::Token));
    if a.rules_start.is_some_and(|s| offset >= s) {
        items.push(item(
            "error",
            CompletionItemKind::KEYWORD,
            Some("The built-in error token, used for error recovery"),
        ));
    }
    items
}

fn item(label: &str, kind: CompletionItemKind, doc: Option<&str>) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(kind),
        documentation: doc.map(|d| {
            Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: d.to_string(),
            })
        }),
        ..Default::default()
    }
}

fn directive_items(table: &[(&str, &str)]) -> Vec<CompletionItem> {
    table
        .iter()
        .map(|(name, doc)| item(name, CompletionItemKind::KEYWORD, Some(doc)))
        .collect()
}

fn symbol_items(a: &Analysis, kind: Kind) -> Vec<CompletionItem> {
    let (lsp_kind, detail) = match kind {
        Kind::Token => (CompletionItemKind::CONSTANT, "token"),
        Kind::Nonterminal => (CompletionItemKind::FUNCTION, "nonterminal"),
        Kind::Definition => (CompletionItemKind::VARIABLE, "definition"),
        Kind::Condition => (CompletionItemKind::ENUM_MEMBER, "start condition"),
    };
    a.symbols
        .iter()
        .filter(|s| s.kind == kind)
        .map(|s| CompletionItem {
            detail: Some(detail.to_string()),
            ..item(&s.name, lsp_kind, Some(&s.hover))
        })
        .collect()
}

fn builtin_hover(ns: Namespace, name: &str) -> Option<String> {
    match (ns, name) {
        (Namespace::Grammar, "error") => {
            Some("`error` — the built-in token used for error recovery.".into())
        }
        (Namespace::Condition, "INITIAL") => {
            Some("`INITIAL` — the default start condition (state 0).".into())
        }
        _ => None,
    }
}

fn valid_name(lang: Lang, name: &str) -> bool {
    let b = name.as_bytes();
    match lang {
        Lang::Bison => {
            !b.is_empty()
                && bison::is_ident_start(b[0])
                && b.iter().all(|&c| bison::is_ident_char(c))
        }
        Lang::Flex => {
            !b.is_empty()
                && (b[0].is_ascii_alphabetic() || b[0] == b'_')
                && b.iter()
                    .all(|&c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        }
    }
}

fn has_extension(path: &Path, exts: &[&str]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| exts.contains(&e))
}

/// Widens an empty span so the editor has something to underline and hover:
/// the next character if there is one on the line, otherwise the last
/// non-blank line before it (e.g. for "missing `%%`" at end of file).
fn visible_span(text: &str, span: &Span) -> Span {
    if !span.is_empty() {
        return span.clone();
    }
    if let Some(c) = text[span.start..]
        .chars()
        .next()
        .filter(|&c| c != '\n' && c != '\r')
    {
        return span.start..span.start + c.len_utf8();
    }
    let before = text[..span.start].trim_end();
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let indent = before[line_start..].len() - before[line_start..].trim_start().len();
    line_start + indent..before.len()
}

fn diagnostics(doc: &Document) -> Vec<lsp_types::Diagnostic> {
    doc.analysis
        .diagnostics
        .iter()
        .map(|d| lsp_types::Diagnostic {
            range: doc.range(&visible_span(&doc.text, &d.span)),
            severity: Some(match d.severity {
                Severity::Error => DiagnosticSeverity::ERROR,
                Severity::Warning => DiagnosticSeverity::WARNING,
            }),
            source: Some("flex-bison-lsp".into()),
            message: d.message.clone(),
            tags: d.unnecessary.then(|| vec![DiagnosticTag::UNNECESSARY]),
            ..Default::default()
        })
        .collect()
}

fn publish_message(uri: Url, diagnostics: Vec<lsp_types::Diagnostic>) -> Message {
    Message::Notification(Notification::new(
        notification::PublishDiagnostics::METHOD.to_string(),
        PublishDiagnosticsParams {
            uri,
            diagnostics,
            version: None,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::visible_span;

    #[test]
    fn empty_spans_become_visible() {
        let text = "%x S\n  last line  \n\n";
        // At end of file: the last non-blank line, without its indentation.
        assert_eq!(
            &text[visible_span(text, &(text.len()..text.len()))],
            "last line"
        );
        // Mid-line: the next character.
        assert_eq!(&text[visible_span(text, &(1..1))], "x");
        // Non-empty spans are unchanged.
        assert_eq!(visible_span(text, &(0..2)), 0..2);
        // Empty file: nothing to widen to.
        assert_eq!(visible_span("", &(0..0)), 0..0);
    }
}
