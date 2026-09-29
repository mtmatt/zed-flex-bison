//! Analysis of Flex scanner files (`.l`).
//!
//! Flex is line-oriented, so the file is processed one line at a time with a
//! little state carried between lines (open `%{` blocks, multi-line actions,
//! start-condition scopes).

use std::collections::{HashMap, HashSet};

use crate::analysis::{Analysis, Namespace, Span, Symbol, SymbolKind};
use crate::docs;

fn is_name_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}

fn is_name_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'-'
}

/// Length of the Flex name at the start of `b` (0 if none).
fn name_len(b: &[u8]) -> usize {
    if !b.first().copied().is_some_and(is_name_start) {
        return 0;
    }
    b.iter().take_while(|&&c| is_name_char(c)).count()
}

/// Scans C code in actions, tracking braces and comments across lines.
#[derive(Default)]
struct CodeScanner {
    depth: i32,
    in_comment: bool,
    /// The previous identifier was `BEGIN` or `yy_push_state`.
    expect_condition: bool,
}

impl CodeScanner {
    fn is_open(&self) -> bool {
        self.depth > 0 || self.in_comment
    }

    fn feed(
        &mut self,
        src: &str,
        span: Span,
        idents: &mut Vec<(String, Span)>,
        conditions: &mut Vec<(String, Span)>,
    ) {
        let b = src.as_bytes();
        let end = span.end;
        let mut i = span.start;
        while i < end {
            let next = if i + 1 < end { b[i + 1] } else { 0 };
            if self.in_comment {
                if b[i] == b'*' && next == b'/' {
                    self.in_comment = false;
                    i += 2;
                } else {
                    i += 1;
                }
                continue;
            }
            match b[i] {
                b'/' if next == b'/' => break,
                b'/' if next == b'*' => {
                    self.in_comment = true;
                    i += 2;
                }
                q @ (b'"' | b'\'') => {
                    i += 1;
                    while i < end && b[i] != q {
                        i += if b[i] == b'\\' { 2 } else { 1 };
                    }
                    i += 1;
                    self.expect_condition = false;
                }
                b'{' | b'}' => {
                    self.depth += if b[i] == b'{' { 1 } else { -1 };
                    self.expect_condition = false;
                    i += 1;
                }
                b'(' => i += 1,
                c if c.is_ascii_whitespace() => i += 1,
                c if is_name_start(c) => {
                    let len = b[i..end]
                        .iter()
                        .take_while(|c| c.is_ascii_alphanumeric() || **c == b'_')
                        .count();
                    let name = &src[i..i + len];
                    let span = i..i + len;
                    if self.expect_condition {
                        conditions.push((name.to_string(), span));
                        self.expect_condition = false;
                    } else if name == "BEGIN" || name == "yy_push_state" {
                        self.expect_condition = true;
                    } else {
                        idents.push((name.to_string(), span));
                    }
                    i += len;
                }
                c if c.is_ascii_digit() => {
                    i += b[i..end]
                        .iter()
                        .take_while(|c| c.is_ascii_alphanumeric())
                        .count();
                    self.expect_condition = false;
                }
                _ => {
                    i += 1;
                    self.expect_condition = false;
                }
            }
        }
    }
}

#[derive(PartialEq, Eq)]
enum Section {
    Definitions,
    Rules,
    UserCode,
}

struct Flex<'a> {
    src: &'a str,
    a: Analysis,
    section: Section,
    /// Start of an open `%{` block.
    c_block: Option<usize>,
    /// Open `%top{` block.
    top_block: Option<(usize, CodeScanner)>,
    /// Inside a `/* */` comment in the definitions section.
    in_comment: bool,
    /// A multi-line action (start offset, scanner).
    action: Option<(usize, CodeScanner)>,
    /// Open `<SC>{` scopes (offset of the `{`).
    scopes: Vec<usize>,
    definitions: HashMap<String, usize>,
    conditions: HashMap<String, usize>,
    definition_refs: Vec<(String, Span)>,
    condition_refs: Vec<(String, Span)>,
}

pub fn analyze(src: &str) -> Analysis {
    let mut f = Flex {
        src,
        a: Analysis::default(),
        section: Section::Definitions,
        c_block: None,
        top_block: None,
        in_comment: false,
        action: None,
        scopes: Vec::new(),
        definitions: HashMap::new(),
        conditions: HashMap::new(),
        definition_refs: Vec::new(),
        condition_refs: Vec::new(),
    };
    let mut start = 0;
    loop {
        let end = src[start..].find('\n').map_or(src.len(), |i| start + i);
        let line = src[start..end].trim_end_matches('\r');
        match f.section {
            Section::Definitions => f.definitions_line(start, line),
            Section::Rules => f.rules_line(start, line),
            Section::UserCode => {
                f.a.code_spans.push(start..src.len());
                break;
            }
        }
        if end == src.len() {
            break;
        }
        start = end + 1;
    }
    f.finish();
    f.a
}

impl Flex<'_> {
    fn eof(&self) -> Span {
        self.src.len()..self.src.len()
    }

    /// Handles lines inside `%{ ... %}`; returns whether the line was consumed.
    fn c_block_line(&mut self, start: usize, line: &str) -> bool {
        let Some(open) = self.c_block else {
            if line.starts_with("%{") {
                self.c_block = Some(start);
                return true;
            }
            return false;
        };
        if line.starts_with("%}") {
            self.a.code_spans.push(open..start);
            self.c_block = None;
        }
        true
    }

    fn feed_code(&mut self, scanner: &mut CodeScanner, span: Span) {
        // Include the newline so a cursor at the end of the line counts as
        // inside the code.
        let with_newline = span.start..(span.end + 1).min(self.src.len());
        self.a.code_spans.push(with_newline);
        scanner.feed(
            self.src,
            span,
            &mut self.a.code_idents,
            &mut self.condition_refs,
        );
    }

    fn definitions_line(&mut self, start: usize, line: &str) {
        let end = start + line.len();
        if self.c_block_line(start, line) {
            return;
        }
        if let Some((open, mut scanner)) = self.top_block.take() {
            self.feed_code(&mut scanner, start..end);
            if scanner.depth > 0 {
                self.top_block = Some((open, scanner));
            }
            return;
        }
        if self.in_comment {
            self.in_comment = !line.contains("*/");
            return;
        }
        let b = line.as_bytes();
        if line.trim().is_empty() {
            return;
        }
        if line.starts_with("%%") {
            self.section = Section::Rules;
            self.a.rules_start = Some(start + 2);
            return;
        }
        if let Some(rest) = line.strip_prefix("%top") {
            let brace = start + 4 + rest.find('{').unwrap_or(0);
            let mut scanner = CodeScanner::default();
            self.feed_code(&mut scanner, brace..end);
            if scanner.depth > 0 {
                self.top_block = Some((start, scanner));
            }
            return;
        }
        if b[0].is_ascii_whitespace() || line.starts_with("/*") {
            // Indented lines are C code copied to the output.
            let trimmed = line.trim_start();
            if trimmed.starts_with("/*") {
                self.in_comment = !trimmed.contains("*/");
            } else {
                self.a.code_spans.push(start..end);
            }
            return;
        }
        if b[0] == b'%' {
            self.directive(start, line);
            return;
        }
        let len = name_len(b);
        if len == 0 {
            self.a
                .error(start..end, "expected a name definition, directive, or `%%`");
            return;
        }
        self.definition(start, line, len);
    }

    fn directive(&mut self, start: usize, line: &str) {
        let b = line.as_bytes();
        let len = 1 + b[1..]
            .iter()
            .take_while(|c| c.is_ascii_alphanumeric())
            .count();
        let directive = &line[..len];
        let span = start..start + len;
        let mut words = Vec::new();
        let mut i = len;
        while i < b.len() {
            if b[i].is_ascii_whitespace() {
                i += 1;
                continue;
            }
            if line[i..].starts_with("/*") {
                break;
            }
            let w = b[i..]
                .iter()
                .take_while(|c| !c.is_ascii_whitespace())
                .count();
            words.push((&line[i..i + w], start + i..start + i + w));
            i += w;
        }
        match directive {
            "%x" | "%s" | "%X" | "%S" => {
                self.a
                    .directives
                    .push((directive.to_ascii_lowercase(), span));
                let exclusive = directive.eq_ignore_ascii_case("%x");
                for (name, span) in words {
                    self.declare_condition(name, span, exclusive);
                }
            }
            "%option" => {
                self.a.directives.push(("%option".into(), span));
                for (word, span) in words {
                    let name = word.split('=').next().unwrap_or(word);
                    let base = name
                        .strip_prefix("no")
                        .filter(|n| docs::flex_option(n).is_some());
                    let key = base.unwrap_or(name);
                    let span = span.start..span.start + name.len();
                    if docs::flex_option(key).is_none() {
                        self.a
                            .warning(span.clone(), format!("unrecognized %option `{name}`"));
                    }
                    self.a.directives.push((format!("option:{key}"), span));
                }
            }
            _ if docs::flex_directive(directive).is_some() => {
                self.a.directives.push((directive.to_string(), span));
            }
            _ => self
                .a
                .warning(span, format!("unrecognized directive `{directive}`")),
        }
    }

    fn declare_condition(&mut self, name: &str, span: Span, exclusive: bool) {
        if name_len(name.as_bytes()) != name.len() {
            self.a
                .error(span, format!("invalid start condition name `{name}`"));
            return;
        }
        self.a
            .occurrence(Namespace::Condition, name, span.clone(), true);
        if self.conditions.contains_key(name) {
            self.a
                .warning(span, format!("start condition `{name}` declared twice"));
            return;
        }
        let (directive, what) = if exclusive {
            ("%x", "Exclusive")
        } else {
            ("%s", "Inclusive")
        };
        self.conditions
            .insert(name.to_string(), self.a.symbols.len());
        self.a.symbols.push(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Condition,
            selection: span.clone(),
            range: span,
            hover: format!("```flex\n{directive} {name}\n```\n{what} start condition"),
        });
    }

    fn definition(&mut self, start: usize, line: &str, len: usize) {
        let name = &line[..len];
        let name_span = start..start + len;
        let rest = &line[len..];
        let body = rest.trim_start();
        if !rest.starts_with([' ', '\t']) || body.is_empty() {
            self.a.error(
                start..start + line.len(),
                format!("incomplete name definition: `{name}` needs a pattern"),
            );
            return;
        }
        let body_start = len + (rest.len() - body.len());
        self.scan_pattern(start, line, body_start, false);
        self.a
            .occurrence(Namespace::Definition, name, name_span.clone(), true);
        if self.definitions.contains_key(name) {
            self.a
                .error(name_span, format!("definition `{name}` is defined twice"));
            return;
        }
        self.definitions
            .insert(name.to_string(), self.a.symbols.len());
        self.a.symbols.push(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Definition,
            selection: name_span,
            range: start..start + line.len(),
            hover: format!(
                "```flex\n{name}  {}\n```\nName definition, used as `{{{name}}}`",
                body.trim_end()
            ),
        });
    }

    /// Scans a pattern starting at `from`, recording `{NAME}` expansions.
    /// Returns the index in `line` where the pattern ends.
    fn scan_pattern(
        &mut self,
        start: usize,
        line: &str,
        from: usize,
        stop_at_space: bool,
    ) -> usize {
        let b = line.as_bytes();
        let mut i = from;
        while i < b.len() {
            match b[i] {
                b' ' | b'\t' if stop_at_space => break,
                b'\\' => i += 2,
                b'"' => {
                    i += 1;
                    while i < b.len() && b[i] != b'"' {
                        i += if b[i] == b'\\' { 2 } else { 1 };
                    }
                    i += 1;
                }
                b'[' => {
                    i += 1;
                    if b.get(i) == Some(&b'^') {
                        i += 1;
                    }
                    if b.get(i) == Some(&b']') {
                        i += 1;
                    }
                    while i < b.len() && b[i] != b']' {
                        if b[i] == b'\\' {
                            i += 2;
                        } else if b[i..].starts_with(b"[:") {
                            i = b[i..]
                                .windows(2)
                                .position(|w| w == b":]")
                                .map_or(i + 1, |k| i + k + 2);
                        } else {
                            i += 1;
                        }
                    }
                    i += 1;
                }
                b'{' => {
                    let len = name_len(&b[i + 1..]);
                    if len > 0 && b.get(i + 1 + len) == Some(&b'}') {
                        let name = &line[i + 1..i + 1 + len];
                        let span = start + i + 1..start + i + 1 + len;
                        self.definition_refs.push((name.to_string(), span.clone()));
                        self.a.occurrence(Namespace::Definition, name, span, false);
                        i += len + 2;
                    } else {
                        // A repetition count such as `{2,3}`.
                        i = line[i..].find('}').map_or(b.len(), |k| i + k + 1);
                    }
                }
                _ => i += 1,
            }
        }
        i.min(b.len())
    }

    fn rules_line(&mut self, start: usize, line: &str) {
        let end = start + line.len();
        if let Some((open, mut scanner)) = self.action.take() {
            self.feed_code(&mut scanner, start..end);
            if scanner.is_open() {
                self.action = Some((open, scanner));
            }
            return;
        }
        if self.c_block_line(start, line) {
            return;
        }
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            return;
        }
        if line.starts_with("%%") {
            self.section = Section::UserCode;
            self.a.code_spans.push(start + 2..self.src.len());
            return;
        }
        if trimmed == "}" && !self.scopes.is_empty() {
            self.scopes.pop();
            return;
        }
        let indent = line.len() - trimmed.len();
        if indent > 0 && self.scopes.is_empty() {
            // Indented code (usually local declarations or comments).
            let mut scanner = CodeScanner::default();
            self.feed_code(&mut scanner, start + indent..end);
            if scanner.is_open() {
                self.action = Some((start + indent, scanner));
            }
            return;
        }
        self.rule(start, line, indent);
    }

    fn rule(&mut self, start: usize, line: &str, from: usize) {
        let b = line.as_bytes();
        let mut i = from;
        if b[i] == b'<' && !line[i..].starts_with("<<EOF>>") {
            match self.scan_conditions(start, line, i) {
                Some(after) => i = after,
                None => {
                    let span = start + i..start + line.len();
                    self.a.error(span, "malformed start condition list");
                    return;
                }
            }
            if line[i..].trim() == "{" {
                self.scopes.push(start + i);
                return;
            }
        }
        let pattern_end = self.scan_pattern(start, line, i, true);
        if pattern_end == i {
            self.a
                .error(start + from..start + line.len(), "missing pattern");
            return;
        }
        let action =
            pattern_end + (line[pattern_end..].len() - line[pattern_end..].trim_start().len());
        let rest = line[action..].trim_end();
        if rest.is_empty() || rest == "|" {
            return;
        }
        let mut scanner = CodeScanner::default();
        self.feed_code(&mut scanner, start + action..start + line.len());
        if scanner.is_open() {
            self.action = Some((start + action, scanner));
        }
    }

    /// Parses `<A,B>` or `<*>` at `from`; returns the index after `>`.
    fn scan_conditions(&mut self, start: usize, line: &str, from: usize) -> Option<usize> {
        let b = line.as_bytes();
        let mut i = from + 1;
        if line[i..].starts_with("*>") {
            return Some(i + 2);
        }
        let mut names = Vec::new();
        loop {
            let len = name_len(&b[i..]);
            if len == 0 {
                return None;
            }
            names.push((line[i..i + len].to_string(), start + i..start + i + len));
            i += len;
            match b.get(i) {
                Some(b',') => i += 1,
                Some(b'>') => break,
                _ => return None,
            }
        }
        for (name, span) in names {
            self.a
                .occurrence(Namespace::Condition, &name, span.clone(), false);
            self.condition_refs.push((name, span));
        }
        Some(i + 1)
    }

    fn finish(&mut self) {
        if let Some(open) = self.c_block {
            self.a
                .error(open..open + 2, "unterminated `%{` block: missing `%}`");
        }
        if let Some((open, _)) = &self.top_block {
            self.a.error(*open..*open + 4, "unterminated `%top` block");
        }
        if let Some((open, _)) = &self.action {
            self.a.error(*open..*open + 1, "unterminated action");
        }
        for &open in &self.scopes {
            self.a.error(
                open..open + 1,
                "unclosed start condition scope: missing `}`",
            );
        }
        if self.section == Section::Definitions {
            self.a
                .error(self.eof(), "missing `%%` before the rules section");
        }

        // Action identifiers that turned out to be conditions (e.g. BEGIN X)
        // were routed to condition_refs by the scanner; record those too.
        let rule_refs: HashSet<Span> = self
            .a
            .occurrences
            .iter()
            .filter(|o| o.ns == Namespace::Condition)
            .map(|o| o.span.clone())
            .collect();
        for (name, span) in &self.condition_refs {
            if !rule_refs.contains(span) {
                self.a
                    .occurrence(Namespace::Condition, name, span.clone(), false);
            }
        }

        let mut used_defs = HashSet::new();
        for (name, span) in std::mem::take(&mut self.definition_refs) {
            if self.definitions.contains_key(&name) {
                used_defs.insert(name);
            } else {
                self.a
                    .error(span, format!("undefined definition `{{{name}}}`"));
            }
        }
        let mut used_conditions = HashSet::new();
        for (name, span) in std::mem::take(&mut self.condition_refs) {
            if self.conditions.contains_key(&name) {
                used_conditions.insert(name);
            } else if name != "INITIAL" {
                self.a
                    .error(span, format!("undeclared start condition `{name}`"));
            }
        }
        let mut unused = Vec::new();
        for (name, &i) in &self.definitions {
            if !used_defs.contains(name) {
                unused.push((
                    self.a.symbols[i].selection.clone(),
                    format!("definition `{name}` is never used"),
                ));
            }
        }
        for (name, &i) in &self.conditions {
            if !used_conditions.contains(name) {
                unused.push((
                    self.a.symbols[i].selection.clone(),
                    format!("start condition `{name}` is never used"),
                ));
            }
        }
        unused.sort_by_key(|(span, _)| span.start);
        for (span, msg) in unused {
            self.a.unused(span, msg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::Severity;

    fn messages(a: &Analysis) -> Vec<String> {
        a.diagnostics.iter().map(|d| d.message.clone()).collect()
    }

    const SCANNER: &str = r#"%{
#include "parser.tab.h"
int depth = 0;
%}
%option noyywrap yylineno
%x COMMENT
%s STRICT

DIGIT    [0-9]
ID       [a-z_][a-z0-9_]*
NUMBER   {DIGIT}+("."{DIGIT}*)?

%%
    /* indented comment */
"/*"            { BEGIN(COMMENT); }
<COMMENT>{
  "*/"          { BEGIN INITIAL; }
  .|\n          ;
}
<STRICT,INITIAL>{NUMBER}  { return NUM; }
{ID}            {
                  yylval.s = strdup(yytext);
                  return ID_TOKEN;
                }
"{"             |
[{}]            { return yytext[0]; }
x{2,3}          ;
<<EOF>>         { return 0; }
%%
int helper(void) { BEGIN(STRICT); return 0; }
"#;

    #[test]
    fn clean_scanner_has_no_diagnostics() {
        let a = analyze(SCANNER);
        assert!(a.diagnostics.is_empty(), "{:?}", messages(&a));
    }

    #[test]
    fn collects_definitions_and_conditions() {
        let a = analyze(SCANNER);
        let digit = a.symbol(Namespace::Definition, "DIGIT").unwrap();
        assert!(digit.hover.contains("DIGIT  [0-9]"));
        let comment = a.symbol(Namespace::Condition, "COMMENT").unwrap();
        assert!(comment.hover.contains("Exclusive"));
        let strict = a.symbol(Namespace::Condition, "STRICT").unwrap();
        assert!(strict.hover.contains("Inclusive"));
        // Definition, and two uses in NUMBER.
        assert_eq!(a.occurrences_of(Namespace::Definition, "DIGIT").count(), 3);
        // Declaration, BEGIN(COMMENT), and the <COMMENT>{ scope.
        assert_eq!(a.occurrences_of(Namespace::Condition, "COMMENT").count(), 3);
        assert_eq!(a.occurrences_of(Namespace::Condition, "INITIAL").count(), 2);
    }

    #[test]
    fn records_action_identifiers() {
        let a = analyze(SCANNER);
        let names: Vec<&str> = a.code_idents.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"NUM"));
        assert!(names.contains(&"ID_TOKEN"), "{names:?}");
        let (_, span) = a.code_idents.iter().find(|(n, _)| n == "NUM").unwrap();
        assert_eq!(&SCANNER[span.clone()], "NUM");
        assert!(a.in_code(SCANNER.find("strdup").unwrap()));
        assert!(!a.in_code(SCANNER.find("{ID}").unwrap() + 1));
    }

    #[test]
    fn records_option_directives() {
        let a = analyze(SCANNER);
        let keys: Vec<&str> = a.directives.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(
            keys,
            ["%option", "option:yywrap", "option:yylineno", "%x", "%s"]
        );
    }

    #[test]
    fn reports_undefined_and_unused_names() {
        let src = "%x A B\nD [0-9]\n%%\n<A,C>{E} { BEGIN(Z); }\n";
        let a = analyze(src);
        let msgs = messages(&a);
        assert_eq!(
            msgs,
            [
                "undefined definition `{E}`",
                "undeclared start condition `C`",
                "undeclared start condition `Z`",
                "start condition `B` is never used",
                "definition `D` is never used",
            ]
        );
        assert!(a.diagnostics[3].unnecessary);
        assert_eq!(a.diagnostics[3].severity, Severity::Warning);
    }

    #[test]
    fn reports_structural_errors() {
        assert_eq!(
            messages(&analyze("%x S\n")),
            [
                "missing `%%` before the rules section",
                "start condition `S` is never used"
            ]
        );
        assert_eq!(
            messages(&analyze("D [0-9]\n{D}\n%%\n{D} ;\n")),
            ["expected a name definition, directive, or `%%`"]
        );
        assert_eq!(
            messages(&analyze("%{\nint x;\n%%\n")),
            [
                "unterminated `%{` block: missing `%}`",
                "missing `%%` before the rules section"
            ]
        );
        assert_eq!(
            messages(&analyze("%%\na { if (x) {\n}\n")),
            ["unterminated action"]
        );
        assert_eq!(
            messages(&analyze("%x S\n%%\n<S>{\na ;\n")),
            ["unclosed start condition scope: missing `}`"]
        );
        assert_eq!(
            messages(&analyze("%option noyywrap bogus\n%%\n")),
            ["unrecognized %option `bogus`"]
        );
        assert_eq!(
            messages(&analyze("D\n%%\n")),
            ["incomplete name definition: `D` needs a pattern"]
        );
        assert_eq!(
            messages(&analyze("D a\nD b\n%%\n{D} ;\n")),
            ["definition `D` is defined twice"]
        );
    }

    #[test]
    fn non_ascii_in_brackets_does_not_panic() {
        for src in [
            "%%\n[àé]+ ;\n",
            "D [\\é]\n%%\n{D} ;\n",
            "%%\n[\\é] ;\n",
            "%%\n[😀[:alpha:]] ;\n",
        ] {
            let a = analyze(src);
            assert!(a.diagnostics.is_empty(), "{src:?}: {:?}", messages(&a));
        }
    }

    #[test]
    fn analyzes_sample_scanner() {
        let src = include_str!("../tests/fixtures/calc.l");
        let a = analyze(src);
        assert!(a.diagnostics.is_empty(), "{:?}", messages(&a));
        assert_eq!(a.occurrences_of(Namespace::Condition, "COMMENT").count(), 6);
    }
}
