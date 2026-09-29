//! Language-independent result of analyzing one Flex or Bison file.

pub use crate::text::Span;

/// Names in different namespaces never refer to each other.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Namespace {
    /// Bison tokens and nonterminals.
    Grammar,
    /// Flex name definitions, used as `{NAME}`.
    Definition,
    /// Flex start conditions.
    Condition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymbolKind {
    Token,
    Nonterminal,
    Definition,
    Condition,
}

impl SymbolKind {
    pub fn namespace(self) -> Namespace {
        match self {
            SymbolKind::Token | SymbolKind::Nonterminal => Namespace::Grammar,
            SymbolKind::Definition => Namespace::Definition,
            SymbolKind::Condition => Namespace::Condition,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    /// The name at its defining site.
    pub selection: Span,
    /// The whole defining construct (rule, definition line, ...).
    pub range: Span,
    /// Markdown shown on hover.
    pub hover: String,
}

/// One textual mention of a symbol.
#[derive(Clone, Debug)]
pub struct Occurrence {
    pub ns: Namespace,
    pub name: String,
    pub span: Span,
    pub is_def: bool,
    /// False for mentions that do not spell the name, e.g. a Bison string
    /// alias `"+"` standing for `PLUS`.
    pub renamable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub span: Span,
    pub severity: Severity,
    pub message: String,
    /// Rendered faded (unused code).
    pub unnecessary: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Analysis {
    pub symbols: Vec<Symbol>,
    pub occurrences: Vec<Occurrence>,
    pub diagnostics: Vec<Diagnostic>,
    /// Directives and options with hover docs, keyed by a docs lookup key.
    pub directives: Vec<(String, Span)>,
    /// Embedded C code (actions, prologue, epilogue).
    pub code_spans: Vec<Span>,
    /// Identifiers inside Flex actions, which may name Bison tokens.
    pub code_idents: Vec<(String, Span)>,
    /// Offset where the rules section (after the first `%%`) starts.
    pub rules_start: Option<usize>,
}

fn contains(span: &Span, offset: usize) -> bool {
    span.start <= offset && offset <= span.end
}

impl Analysis {
    pub fn symbol(&self, ns: Namespace, name: &str) -> Option<&Symbol> {
        self.symbols
            .iter()
            .find(|s| s.kind.namespace() == ns && s.name == name)
    }

    pub fn occurrence_at(&self, offset: usize) -> Option<&Occurrence> {
        self.occurrences.iter().find(|o| contains(&o.span, offset))
    }

    pub fn occurrences_of<'a>(
        &'a self,
        ns: Namespace,
        name: &'a str,
    ) -> impl Iterator<Item = &'a Occurrence> + 'a {
        self.occurrences
            .iter()
            .filter(move |o| o.ns == ns && o.name == name)
    }

    pub fn directive_at(&self, offset: usize) -> Option<&(String, Span)> {
        self.directives.iter().find(|(_, s)| contains(s, offset))
    }

    pub fn code_ident_at(&self, offset: usize) -> Option<&(String, Span)> {
        self.code_idents.iter().find(|(_, s)| contains(s, offset))
    }

    /// Whether `offset` is strictly inside embedded C code.
    pub fn in_code(&self, offset: usize) -> bool {
        self.code_spans
            .iter()
            .any(|s| s.start < offset && offset < s.end)
    }

    pub(crate) fn error(&mut self, span: Span, message: impl Into<String>) {
        self.push_diag(span, Severity::Error, message, false);
    }

    pub(crate) fn warning(&mut self, span: Span, message: impl Into<String>) {
        self.push_diag(span, Severity::Warning, message, false);
    }

    pub(crate) fn unused(&mut self, span: Span, message: impl Into<String>) {
        self.push_diag(span, Severity::Warning, message, true);
    }

    fn push_diag(
        &mut self,
        span: Span,
        severity: Severity,
        message: impl Into<String>,
        unnecessary: bool,
    ) {
        self.diagnostics.push(Diagnostic {
            span,
            severity,
            message: message.into(),
            unnecessary,
        });
    }

    pub(crate) fn occurrence(&mut self, ns: Namespace, name: &str, span: Span, is_def: bool) {
        self.occurrences.push(Occurrence {
            ns,
            name: name.to_string(),
            span,
            is_def,
            renamable: true,
        });
    }
}
