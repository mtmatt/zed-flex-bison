//! Analysis of Bison grammar files (`.y`).

use std::collections::{HashMap, HashSet, VecDeque};

use crate::analysis::{Analysis, Namespace, Occurrence, Span, Symbol, SymbolKind};
use crate::docs;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tok {
    Ident,
    Directive,
    Tag,
    Char,
    Str,
    Number,
    Code,
    Prologue,
    Punct(u8),
    /// `%%`
    Sep,
    /// `[name]` after a symbol.
    NamedRef,
}

#[derive(Clone, Debug)]
struct Token {
    kind: Tok,
    span: Span,
}

pub fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c == b'.' || c >= 0x80
}

pub fn is_ident_char(c: u8) -> bool {
    is_ident_start(c) || c.is_ascii_digit() || c == b'-'
}

struct Lexer<'a> {
    b: &'a [u8],
    pos: usize,
    a: &'a mut Analysis,
}

impl Lexer<'_> {
    fn peek(&self, n: usize) -> u8 {
        self.b.get(self.pos + n).copied().unwrap_or(0)
    }

    fn skip_trivia(&mut self) {
        while self.pos < self.b.len() {
            match (self.peek(0), self.peek(1)) {
                (c, _) if c.is_ascii_whitespace() => self.pos += 1,
                (b'/', b'/') => self.pos = self.line_end(self.pos),
                (b'/', b'*') => self.pos = self.skip_block_comment(self.pos),
                _ => break,
            }
        }
    }

    fn line_end(&self, from: usize) -> usize {
        self.b[from..]
            .iter()
            .position(|&c| c == b'\n')
            .map_or(self.b.len(), |i| from + i)
    }

    fn find(&self, from: usize, needle: &[u8]) -> Option<usize> {
        self.b[from..]
            .windows(needle.len())
            .position(|w| w == needle)
            .map(|i| from + i)
    }

    /// `from` points at `/*`; returns the offset after `*/`.
    fn skip_block_comment(&mut self, from: usize) -> usize {
        match self.find(from + 2, b"*/") {
            Some(i) => i + 2,
            None => {
                self.a.error(from..from + 2, "unterminated comment");
                self.b.len()
            }
        }
    }

    /// `from` points at a quote; returns the offset after the closing quote.
    fn skip_quoted(&mut self, from: usize) -> usize {
        let quote = self.b[from];
        let mut i = from + 1;
        while i < self.b.len() {
            match self.b[i] {
                b'\\' => i += 2,
                b'\n' => break,
                c if c == quote => return i + 1,
                _ => i += 1,
            }
        }
        let end = i.min(self.b.len());
        let what = if quote == b'"' { "string" } else { "character" };
        self.a
            .error(from..end, format!("unterminated {what} literal"));
        end
    }

    /// `from` points at `{`; returns the offset after the matching `}`.
    fn skip_code(&mut self, from: usize) -> usize {
        let mut depth = 0usize;
        let mut i = from;
        while i < self.b.len() {
            match (self.b[i], self.b.get(i + 1).copied().unwrap_or(0)) {
                (b'{', _) => {
                    depth += 1;
                    i += 1;
                }
                (b'}', _) => {
                    depth -= 1;
                    i += 1;
                    if depth == 0 {
                        return i;
                    }
                }
                (b'"' | b'\'', _) => {
                    // Apostrophes in code are almost always char literals; a
                    // stray one must not swallow the rest of the file.
                    let q = self.b[i];
                    let mut j = i + 1;
                    while j < self.b.len() && self.b[j] != q && self.b[j] != b'\n' {
                        j += if self.b[j] == b'\\' { 2 } else { 1 };
                    }
                    i = (j + 1).min(self.b.len());
                }
                (b'/', b'/') => i = self.line_end(i),
                (b'/', b'*') => i = self.skip_block_comment(i),
                _ => i += 1,
            }
        }
        self.a
            .error(from..from + 1, "unterminated code block: missing `}`");
        self.b.len()
    }

    /// `from` points at `<`; returns the offset after the matching `>` on the
    /// same line, if any.
    fn scan_tag(&self, from: usize) -> Option<usize> {
        let mut depth = 0usize;
        for i in from..self.b.len() {
            match self.b[i] {
                b'\n' => return None,
                b'<' => depth += 1,
                b'>' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i + 1);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn next(&mut self) -> Option<Token> {
        self.skip_trivia();
        let start = self.pos;
        let c = *self.b.get(start)?;
        let kind = match c {
            b'%' => match self.peek(1) {
                b'%' => {
                    self.pos += 2;
                    Tok::Sep
                }
                b'{' => {
                    self.pos = match self.find(start + 2, b"%}") {
                        Some(i) => i + 2,
                        None => {
                            self.a
                                .error(start..start + 2, "unterminated `%{` block: missing `%}`");
                            self.b.len()
                        }
                    };
                    Tok::Prologue
                }
                _ => {
                    let len = self.b[start + 1..]
                        .iter()
                        .take_while(|&&c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
                        .count();
                    self.pos += 1 + len;
                    if len > 0 {
                        Tok::Directive
                    } else {
                        Tok::Punct(b'%')
                    }
                }
            },
            b'{' => {
                self.pos = self.skip_code(start);
                Tok::Code
            }
            b'<' => match self.scan_tag(start) {
                Some(end) => {
                    self.pos = end;
                    Tok::Tag
                }
                None => {
                    self.pos += 1;
                    Tok::Punct(c)
                }
            },
            b'\'' => {
                self.pos = self.skip_quoted(start);
                Tok::Char
            }
            b'"' => {
                self.pos = self.skip_quoted(start);
                Tok::Str
            }
            b'[' => {
                let end = self.line_end(start);
                match self.b[start..end].iter().position(|&c| c == b']') {
                    Some(i) => {
                        self.pos = start + i + 1;
                        Tok::NamedRef
                    }
                    None => {
                        self.pos += 1;
                        Tok::Punct(c)
                    }
                }
            }
            c if c.is_ascii_digit() => {
                self.pos += self.b[start..]
                    .iter()
                    .take_while(|c| c.is_ascii_alphanumeric())
                    .count();
                Tok::Number
            }
            c if is_ident_start(c) => {
                self.pos += self.b[start..]
                    .iter()
                    .take_while(|&&c| is_ident_char(c))
                    .count();
                Tok::Ident
            }
            _ => {
                self.pos += 1;
                Tok::Punct(c)
            }
        };
        Some(Token {
            kind,
            span: start..self.pos,
        })
    }
}

enum Level {
    Error,
    Warning,
    Unused,
}

#[derive(Default)]
struct Alt {
    /// Components as written, for hover.
    text: Vec<String>,
    /// Identifiers on the right-hand side.
    syms: Vec<String>,
}

struct Rule {
    name: String,
    name_span: Span,
    range: Span,
    alts: Vec<Alt>,
}

#[derive(Default)]
struct Decl {
    first: Option<Span>,
    lines: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// `%token`, `%left`, ...: identifiers declare tokens.
    Token,
    /// `%nterm`: identifiers declare nonterminals.
    Nterm,
    /// `%type`, `%destructor`, ...: identifiers reference symbols.
    Ref,
    /// `%start`: the next identifier is the start symbol.
    Start,
    Other,
}

fn mode_for(directive: &str) -> Mode {
    match directive {
        "%token" | "%left" | "%right" | "%nonassoc" | "%precedence" => Mode::Token,
        "%nterm" => Mode::Nterm,
        "%type" | "%destructor" | "%printer" => Mode::Ref,
        "%start" => Mode::Start,
        _ => Mode::Other,
    }
}

pub fn analyze(src: &str) -> Analysis {
    let mut a = Analysis::default();
    let mut toks = Vec::new();
    let mut lx = Lexer {
        b: src.as_bytes(),
        pos: 0,
        a: &mut a,
    };
    let mut seps = 0;
    while let Some(t) = lx.next() {
        let is_sep = t.kind == Tok::Sep;
        toks.push(t);
        if is_sep {
            seps += 1;
            if seps == 2 {
                break;
            }
        }
    }
    let epilogue_start = lx.pos;
    let mut p = Parser {
        src,
        a,
        tokens: HashMap::new(),
        token_order: Vec::new(),
        nterms: HashMap::new(),
        nterm_order: Vec::new(),
        type_lines: HashMap::new(),
        aliases: HashMap::new(),
        rules: Vec::new(),
        refs: Vec::new(),
        start: None,
    };
    p.parse(&toks, epilogue_start);
    p.a
}

struct Parser<'a> {
    src: &'a str,
    a: Analysis,
    tokens: HashMap<String, Decl>,
    token_order: Vec<String>,
    nterms: HashMap<String, Decl>,
    nterm_order: Vec<String>,
    type_lines: HashMap<String, Vec<String>>,
    /// String alias (with quotes) -> token name.
    aliases: HashMap<String, String>,
    rules: Vec<Rule>,
    /// Symbol references that must resolve.
    refs: Vec<(String, Span)>,
    start: Option<(String, Span)>,
}

impl Parser<'_> {
    fn text(&self, span: &Span) -> &str {
        &self.src[span.clone()]
    }

    fn alias_ref(&mut self, span: &Span) {
        if let Some(name) = self.aliases.get(self.text(span)) {
            self.a.occurrences.push(Occurrence {
                ns: Namespace::Grammar,
                name: name.clone(),
                span: span.clone(),
                is_def: false,
                renamable: false,
            });
        }
    }

    fn parse(&mut self, toks: &[Token], epilogue_start: usize) {
        let Some(sep) = self.declarations(toks) else {
            let end = self.src.len();
            self.a
                .error(end..end, "missing `%%` before the grammar rules");
            self.resolve();
            return;
        };
        self.a.rules_start = Some(toks[sep].span.end);
        let closed = self.rules_section(&toks[sep + 1..]);
        if self.rules.is_empty() {
            self.a
                .error(toks[sep].span.clone(), "no rules in the input grammar");
        }
        if closed {
            self.a.code_spans.push(epilogue_start..self.src.len());
        }
        self.resolve();
    }

    /// Returns the index of the `%%` ending the declarations.
    fn declarations(&mut self, toks: &[Token]) -> Option<usize> {
        let mut mode = Mode::Other;
        let mut directive = String::new();
        let mut tag: Option<String> = None;
        let mut last_token: Option<String> = None;
        for (i, t) in toks.iter().enumerate() {
            let text = self.text(&t.span).to_string();
            match t.kind {
                Tok::Sep => return Some(i),
                Tok::Directive => {
                    mode = mode_for(&text);
                    tag = None;
                    last_token = None;
                    if docs::bison_directive(&text).is_none() {
                        self.a
                            .warning(t.span.clone(), format!("unrecognized directive `{text}`"));
                    }
                    self.a.directives.push((text.clone(), t.span.clone()));
                    directive = text;
                }
                Tok::Tag => tag = Some(text),
                Tok::Ident => {
                    let line = match &tag {
                        Some(tag) => format!("{directive} {tag} {text}"),
                        None => format!("{directive} {text}"),
                    };
                    match mode {
                        Mode::Token => {
                            let decl = self.tokens.entry(text.clone()).or_default();
                            if decl.first.is_none() {
                                decl.first = Some(t.span.clone());
                                self.token_order.push(text.clone());
                            }
                            decl.lines.push(line);
                            self.a
                                .occurrence(Namespace::Grammar, &text, t.span.clone(), true);
                            last_token = Some(text);
                        }
                        Mode::Nterm => {
                            let decl = self.nterms.entry(text.clone()).or_default();
                            if decl.first.is_none() {
                                decl.first = Some(t.span.clone());
                                self.nterm_order.push(text.clone());
                            }
                            decl.lines.push(line);
                            self.a
                                .occurrence(Namespace::Grammar, &text, t.span.clone(), true);
                        }
                        Mode::Ref => {
                            if directive == "%type" {
                                self.type_lines.entry(text.clone()).or_default().push(line);
                            }
                            self.reference(&text, &t.span);
                        }
                        Mode::Start => {
                            self.start = Some((text.clone(), t.span.clone()));
                            self.reference(&text, &t.span);
                            mode = Mode::Other;
                        }
                        Mode::Other => {}
                    }
                }
                Tok::Str => match (mode, last_token.take()) {
                    (Mode::Token, Some(name)) if directive == "%token" => {
                        if let Some(line) =
                            self.tokens.get_mut(&name).and_then(|d| d.lines.last_mut())
                        {
                            line.push(' ');
                            line.push_str(&text);
                        }
                        self.aliases.insert(text, name);
                    }
                    _ => self.alias_ref(&t.span),
                },
                Tok::Char | Tok::Number => {}
                Tok::Code | Tok::Prologue => self.a.code_spans.push(t.span.clone()),
                Tok::Punct(b';') => mode = Mode::Other,
                _ => {}
            }
        }
        None
    }

    fn reference(&mut self, name: &str, span: &Span) {
        self.refs.push((name.to_string(), span.clone()));
        self.a
            .occurrence(Namespace::Grammar, name, span.clone(), false);
    }

    /// Returns whether a closing `%%` was found.
    fn rules_section(&mut self, toks: &[Token]) -> bool {
        let mut cur: Option<usize> = None;
        let mut i = 0;
        while i < toks.len() {
            let t = &toks[i];
            let text = self.text(&t.span).to_string();
            if t.kind == Tok::Sep {
                return true;
            }
            if let Some(r) = cur {
                self.rules[r].range.end = t.span.end;
            }
            match t.kind {
                Tok::Ident => {
                    let mut j = i + 1;
                    if toks.get(j).is_some_and(|t| t.kind == Tok::NamedRef) {
                        j += 1;
                    }
                    if toks.get(j).is_some_and(|t| t.kind == Tok::Punct(b':')) {
                        self.a
                            .occurrence(Namespace::Grammar, &text, t.span.clone(), true);
                        self.rules.push(Rule {
                            name: text,
                            name_span: t.span.clone(),
                            range: t.span.start..toks[j].span.end,
                            alts: vec![Alt::default()],
                        });
                        cur = Some(self.rules.len() - 1);
                        i = j + 1;
                        continue;
                    }
                    self.reference(&text, &t.span);
                    match cur {
                        Some(r) => {
                            let alt = self.rules[r].alts.last_mut().unwrap();
                            alt.text.push(text.clone());
                            alt.syms.push(text);
                        }
                        None => self
                            .a
                            .error(t.span.clone(), "expected a rule: `name: components ;`"),
                    }
                }
                Tok::Char | Tok::Str => {
                    if t.kind == Tok::Str {
                        self.alias_ref(&t.span);
                    }
                    if let Some(r) = cur {
                        self.rules[r].alts.last_mut().unwrap().text.push(text);
                    }
                }
                Tok::Punct(b'|') => match cur {
                    Some(r) => self.rules[r].alts.push(Alt::default()),
                    None => self.a.error(t.span.clone(), "`|` outside of a rule"),
                },
                Tok::Punct(b';') => cur = None,
                Tok::Code => self.a.code_spans.push(t.span.clone()),
                Tok::Directive => {
                    self.a.directives.push((text.clone(), t.span.clone()));
                    if text == "%prec" {
                        if let Some(next) = toks.get(i + 1).filter(|t| t.kind == Tok::Ident) {
                            let name = self.text(&next.span).to_string();
                            self.reference(&name, &next.span);
                            if let Some(r) = cur {
                                self.rules[r].range.end = next.span.end;
                                let alt = self.rules[r].alts.last_mut().unwrap();
                                alt.text.push(format!("%prec {name}"));
                            }
                            i += 1;
                        }
                    } else if text == "%empty" {
                        if let Some(r) = cur {
                            self.rules[r].alts.last_mut().unwrap().text.push(text);
                        }
                    }
                }
                Tok::Punct(c) => self.a.error(
                    t.span.clone(),
                    format!("unexpected `{}` in grammar rules", c as char),
                ),
                _ => {}
            }
            i += 1;
        }
        false
    }

    fn rules_by_name(&self) -> HashMap<&str, Vec<usize>> {
        let mut by_name: HashMap<&str, Vec<usize>> = HashMap::new();
        for (i, r) in self.rules.iter().enumerate() {
            by_name.entry(r.name.as_str()).or_default().push(i);
        }
        by_name
    }

    fn resolve(&mut self) {
        let by_name = self.rules_by_name();
        let mut symbols = Vec::new();
        let mut diags: Vec<(Span, String, Level)> = Vec::new();

        for name in &self.token_order {
            if by_name.contains_key(name.as_str()) {
                continue;
            }
            let decl = &self.tokens[name];
            let span = decl.first.clone().unwrap();
            symbols.push(Symbol {
                name: name.clone(),
                kind: SymbolKind::Token,
                selection: span.clone(),
                range: span,
                hover: format!("```bison\n{}\n```\nToken `{name}`", decl.lines.join("\n")),
            });
        }

        let mut seen = HashSet::new();
        for rule in &self.rules {
            let name = rule.name.as_str();
            if self.tokens.contains_key(name) {
                diags.push((
                    rule.name_span.clone(),
                    format!("rule given for `{name}`, which is a token"),
                    Level::Error,
                ));
            }
            if seen.insert(name) {
                symbols.push(Symbol {
                    name: name.to_string(),
                    kind: SymbolKind::Nonterminal,
                    selection: rule.name_span.clone(),
                    range: rule.range.clone(),
                    hover: self.nonterminal_hover(name, &by_name[name]),
                });
            }
        }

        for name in &self.nterm_order {
            if by_name.contains_key(name.as_str()) {
                continue;
            }
            let decl = &self.nterms[name];
            let span = decl.first.clone().unwrap();
            diags.push((
                span.clone(),
                format!("nonterminal `{name}` has no rules"),
                Level::Warning,
            ));
            symbols.push(Symbol {
                name: name.clone(),
                kind: SymbolKind::Nonterminal,
                selection: span.clone(),
                range: span,
                hover: format!(
                    "```bison\n{}\n```\nNonterminal `{name}` (no rules)",
                    decl.lines.join("\n")
                ),
            });
        }

        for (name, span) in &self.refs {
            let defined = name == "error"
                || self.tokens.contains_key(name)
                || by_name.contains_key(name.as_str())
                || self.nterms.contains_key(name);
            if !defined {
                diags.push((
                    span.clone(),
                    format!(
                        "symbol `{name}` is used, but is not defined as a token and has no rules"
                    ),
                    Level::Error,
                ));
            }
        }

        let start = self.start.clone().or_else(|| {
            self.rules
                .first()
                .map(|r| (r.name.clone(), r.name_span.clone()))
        });
        if let Some((start, start_span)) = start {
            if self.tokens.contains_key(&start) && !by_name.contains_key(start.as_str()) {
                diags.push((
                    start_span,
                    format!("the start symbol `{start}` is a token"),
                    Level::Error,
                ));
            } else if by_name.contains_key(start.as_str()) {
                self.check_usefulness(&start, &by_name, &mut diags);
            }
        }

        for (span, msg, level) in diags {
            match level {
                Level::Error => self.a.error(span, msg),
                Level::Warning => self.a.warning(span, msg),
                Level::Unused => self.a.unused(span, msg),
            }
        }
        self.a.symbols = symbols;
    }

    /// Reports nonterminals unreachable from `start` or unable to derive any
    /// string of tokens.
    fn check_usefulness(
        &self,
        start: &str,
        by_name: &HashMap<&str, Vec<usize>>,
        diags: &mut Vec<(Span, String, Level)>,
    ) {
        let mut reachable = HashSet::from([start]);
        let mut queue = VecDeque::from([start]);
        while let Some(n) = queue.pop_front() {
            for &r in &by_name[n] {
                for sym in self.rules[r].alts.iter().flat_map(|a| &a.syms) {
                    if by_name.contains_key(sym.as_str()) && reachable.insert(sym.as_str()) {
                        queue.push_back(sym.as_str());
                    }
                }
            }
        }

        let mut productive: HashSet<&str> = HashSet::new();
        loop {
            let before = productive.len();
            for rule in &self.rules {
                if productive.contains(rule.name.as_str()) {
                    continue;
                }
                let derives = rule.alts.iter().any(|alt| {
                    alt.syms.iter().all(|s| {
                        !by_name.contains_key(s.as_str()) || productive.contains(s.as_str())
                    })
                });
                if derives {
                    productive.insert(rule.name.as_str());
                }
            }
            if productive.len() == before {
                break;
            }
        }

        for rule in &self.rules {
            let name = rule.name.as_str();
            let span = rule.name_span.clone();
            if !reachable.contains(name) {
                diags.push((
                    span,
                    format!("nonterminal `{name}` is useless: unreachable from the start symbol `{start}`"),
                    Level::Unused,
                ));
            } else if !productive.contains(name) {
                if name == start {
                    let msg = format!("start symbol `{name}` does not derive any sentence");
                    diags.push((span, msg, Level::Error));
                } else {
                    let msg = format!("nonterminal `{name}` never derives a string of tokens");
                    diags.push((span, msg, Level::Warning));
                }
            }
        }
    }

    fn nonterminal_hover(&self, name: &str, rule_ids: &[usize]) -> String {
        const MAX_ALTS: usize = 30;
        let mut out = String::from("```bison\n");
        for line in self.type_lines.get(name).into_iter().flatten() {
            out.push_str(line);
            out.push('\n');
        }
        let pad = " ".repeat(name.len());
        let alts: Vec<&Alt> = rule_ids.iter().flat_map(|&r| &self.rules[r].alts).collect();
        for (i, alt) in alts.iter().take(MAX_ALTS).enumerate() {
            let body = if alt.text.is_empty() {
                "%empty".to_string()
            } else {
                alt.text.join(" ")
            };
            let lead = if i == 0 {
                format!("{name}:")
            } else {
                format!("{pad}|")
            };
            out.push_str(&format!("{lead} {body}\n"));
        }
        if alts.len() > MAX_ALTS {
            out.push_str(&format!("{pad}| /* {} more */\n", alts.len() - MAX_ALTS));
        }
        out.push_str(&format!("{pad};\n```\nNonterminal `{name}`"));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::Severity;

    fn messages(a: &Analysis) -> Vec<String> {
        a.diagnostics.iter().map(|d| d.message.clone()).collect()
    }

    const CALC: &str = r#"%{
#include <stdio.h>
int yylex(void);
%}
%union { int ival; }
%token <ival> NUM "number"
%token PLUS '+'
%left PLUS
%type <ival> expr
%%
input: %empty | input line ;
line: expr '\n' { printf("%d\n", $1); } ;
expr: NUM
    | expr PLUS expr
    | "number" %prec PLUS
    ;
%%
int main(void) { return yyparse(); }
"#;

    #[test]
    fn clean_grammar_has_no_diagnostics() {
        let a = analyze(CALC);
        assert!(a.diagnostics.is_empty(), "{:?}", messages(&a));
    }

    #[test]
    fn collects_tokens_and_nonterminals() {
        let a = analyze(CALC);
        let num = a.symbol(Namespace::Grammar, "NUM").unwrap();
        assert_eq!(num.kind, SymbolKind::Token);
        assert!(num.hover.contains("%token <ival> NUM \"number\""));
        let expr = a.symbol(Namespace::Grammar, "expr").unwrap();
        assert_eq!(expr.kind, SymbolKind::Nonterminal);
        assert_eq!(&CALC[expr.selection.clone()], "expr");
        assert!(expr.hover.contains("%type <ival> expr"));
        assert!(expr.hover.contains("expr: NUM\n    | expr PLUS expr"));
        assert!(expr.hover.contains("| \"number\" %prec PLUS"));
    }

    #[test]
    fn records_references_including_aliases() {
        let a = analyze(CALC);
        let num: Vec<_> = a.occurrences_of(Namespace::Grammar, "NUM").collect();
        // Declaration, use in `expr: NUM`, and the `"number"` alias.
        assert_eq!(num.len(), 3);
        assert_eq!(num.iter().filter(|o| !o.renamable).count(), 1);
        let plus = a.occurrences_of(Namespace::Grammar, "PLUS").count();
        // %token, %left, rule, %prec.
        assert_eq!(plus, 4);
    }

    #[test]
    fn code_is_not_analyzed() {
        let a = analyze(CALC);
        let printf = CALC.find("printf(\"%d").unwrap();
        assert!(a.in_code(printf));
        assert!(a.occurrence_at(printf).is_none());
        assert!(a.in_code(CALC.find("yyparse").unwrap()));
    }

    #[test]
    fn reports_undefined_symbols() {
        let a = analyze("%token A\n%%\ns: A B | C ;\n");
        let msgs = messages(&a);
        assert_eq!(msgs.len(), 2, "{msgs:?}");
        assert!(msgs[0].contains("`B` is used"));
        assert!(a.diagnostics.iter().all(|d| d.severity == Severity::Error));
    }

    #[test]
    fn reports_useless_nonterminals() {
        let a = analyze("%token A\n%%\ns: A ;\nunused: A ;\nloop: loop A ;\n");
        let msgs = messages(&a);
        assert!(
            msgs.iter().any(|m| m.contains("`unused` is useless")),
            "{msgs:?}"
        );
        assert!(
            msgs.iter().any(|m| m.contains("`loop` is useless")),
            "{msgs:?}"
        );
        let a = analyze("%token A\n%%\ns: s A ;\n");
        assert_eq!(
            messages(&a),
            ["start symbol `s` does not derive any sentence"]
        );
    }

    #[test]
    fn respects_start_directive() {
        let a = analyze("%token A\n%start b\n%%\na: A ;\nb: A ;\n");
        let msgs = messages(&a);
        assert_eq!(
            msgs,
            ["nonterminal `a` is useless: unreachable from the start symbol `b`"]
        );
        assert!(a.diagnostics[0].unnecessary);
    }

    #[test]
    fn reports_structural_errors() {
        assert_eq!(
            messages(&analyze("%token A\n")),
            ["missing `%%` before the grammar rules"]
        );
        assert_eq!(
            messages(&analyze("%token A\n%%\n")),
            ["no rules in the input grammar"]
        );
        let msgs = messages(&analyze("%token A\n%%\nA: A ;\n"));
        assert!(
            msgs.contains(&"rule given for `A`, which is a token".to_string()),
            "{msgs:?}"
        );
        let msgs = messages(&analyze("%tokn A\n%%\ns: %empty ;\n"));
        assert_eq!(msgs, ["unrecognized directive `%tokn`"]);
        let msgs = messages(&analyze("%%\ns: { if (x) ;\n"));
        assert_eq!(msgs, ["unterminated code block: missing `}`"]);
        let msgs = messages(&analyze("%nterm n\n%%\ns: %empty ;\n"));
        assert_eq!(msgs, ["nonterminal `n` has no rules"]);
    }

    #[test]
    fn only_token_declarations_define_aliases() {
        let src = "%token NUM \"number\"\n%left NUM \"+\"\n%%\ns: NUM \"+\" \"number\" ;\n";
        let a = analyze(src);
        let aliases: Vec<&str> = a
            .occurrences_of(Namespace::Grammar, "NUM")
            .filter(|o| !o.renamable)
            .map(|o| &src[o.span.clone()])
            .collect();
        assert_eq!(aliases, ["\"number\""]);
        let hover = &a.symbol(Namespace::Grammar, "NUM").unwrap().hover;
        assert!(hover.contains("%left NUM\n"), "{hover}");
    }

    #[test]
    fn nterms_without_rules_keep_declaration_order() {
        let a = analyze("%nterm c b a\n%%\ns: %empty ;\n");
        let names: Vec<&str> = a.symbols.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["s", "c", "b", "a"]);
    }

    #[test]
    fn rule_without_semicolon_before_next_rule() {
        let a = analyze("%token A\n%%\ns: t\nt: A\n");
        assert!(a.diagnostics.is_empty(), "{:?}", messages(&a));
        assert_eq!(a.symbols.len(), 3);
    }

    #[test]
    fn tolerates_braces_and_comments_in_actions() {
        let src = "%%\ns: %empty { /* } */ char c = '}'; puts(\"}\"); } ;\n";
        let a = analyze(src);
        assert!(a.diagnostics.is_empty(), "{:?}", messages(&a));
    }
}
