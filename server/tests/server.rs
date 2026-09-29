use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use flex_bison_lsp::server::State;
use lsp_server::Notification;
use lsp_types::*;
use serde_json::{json, Value};

const PARSER: &str = include_str!("fixtures/calc.y");
const LEXER: &str = include_str!("fixtures/calc.l");

fn uri(name: &str) -> Url {
    Url::parse(&format!("file:///nonexistent/{name}")).unwrap()
}

fn open(state: &mut State, name: &str, language_id: &str, text: &str) -> Vec<Diagnostic> {
    let out = state.handle_notification(Notification::new(
        "textDocument/didOpen".into(),
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem::new(uri(name), language_id.into(), 1, text.into()),
        },
    ));
    let [lsp_server::Message::Notification(n)] = out.as_slice() else {
        panic!("expected one notification, got {out:?}");
    };
    serde_json::from_value::<PublishDiagnosticsParams>(n.params.clone())
        .unwrap()
        .diagnostics
}

/// Position of the `nth` occurrence of `needle` in `text`, plus `delta` columns.
fn pos(text: &str, needle: &str, nth: usize, delta: u32) -> Position {
    let offset = text.match_indices(needle).nth(nth).unwrap().0;
    let line = text[..offset].matches('\n').count() as u32;
    let col = (offset - text[..offset].rfind('\n').map_or(0, |i| i + 1)) as u32;
    Position::new(line, col + delta)
}

fn hover_text(h: Hover) -> String {
    match h.contents {
        HoverContents::Markup(m) => m.value,
        other => panic!("unexpected hover {other:?}"),
    }
}

fn labels(c: Option<CompletionResponse>) -> Vec<String> {
    match c {
        Some(CompletionResponse::Array(items)) => items.into_iter().map(|i| i.label).collect(),
        other => panic!("unexpected completion {other:?}"),
    }
}

#[test]
fn sample_parser_reports_undeclared_tokens() {
    let mut state = State::default();
    let diags = open(&mut state, "calc.y", "bison", PARSER);
    let msgs: Vec<&str> = diags.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(
        msgs,
        [
            "symbol `ASSIGN` is used, but is not defined as a token and has no rules",
            "symbol `COMMA` is used, but is not defined as a token and has no rules",
        ]
    );
    assert_eq!(diags[0].range.start, pos(PARSER, "IDENT ASSIGN", 0, 6));
}

#[test]
fn bison_navigation() {
    let mut state = State::default();
    open(&mut state, "calc.y", "bison", PARSER);
    let u = uri("calc.y");

    // `stmt` in `stmt_list : stmt_list stmt` goes to its rule.
    let at = pos(PARSER, "stmt_list stmt", 0, 10);
    let Some(GotoDefinitionResponse::Scalar(loc)) = state.definition(&u, at) else {
        panic!()
    };
    assert_eq!(loc.range.start, pos(PARSER, "stmt :", 0, 0));

    let refs = state
        .references(&u, pos(PARSER, "LPAREN", 0, 0), true)
        .unwrap();
    assert_eq!(refs.len(), 3);
    let refs = state
        .references(&u, pos(PARSER, "LPAREN", 0, 0), false)
        .unwrap();
    assert_eq!(refs.len(), 2);

    let hover = hover_text(state.hover(&u, pos(PARSER, "args RPAREN", 0, 0)).unwrap());
    assert!(
        hover.contains("args: args COMMA expr\n    | expr"),
        "{hover}"
    );
    let hover = hover_text(state.hover(&u, pos(PARSER, "%right", 0, 2)).unwrap());
    assert!(hover.contains("right-associative"));

    let edit = state
        .rename(&u, pos(PARSER, "SEMICOLON", 0, 0), "SEMI")
        .unwrap()
        .unwrap();
    // %token, and twice in `stmt`.
    assert_eq!(edit.changes.unwrap()[&u].len(), 3);
    assert_eq!(
        state.rename(&u, pos(PARSER, "SEMICOLON", 0, 0), "not valid"),
        Err("`not valid` is not a valid name".to_string())
    );
    assert_eq!(state.rename(&u, Position::new(0, 0), "x"), Ok(None));

    let Some(DocumentSymbolResponse::Nested(symbols)) = state.document_symbols(&u) else {
        panic!()
    };
    assert!(symbols
        .iter()
        .any(|s| s.name == "expr" && s.kind == SymbolKind::FUNCTION));
    assert!(symbols
        .iter()
        .any(|s| s.name == "NUMBER" && s.kind == SymbolKind::CONSTANT));
}

#[test]
fn bison_completion() {
    let mut state = State::default();
    let text = "%tok\n%token NUM\n%%\ns: N ;\n";
    open(&mut state, "c.y", "bison", text);
    let u = uri("c.y");
    let directives = labels(state.completion(&u, Position::new(0, 4)));
    assert!(directives.contains(&"%token".to_string()));
    let symbols = labels(state.completion(&u, Position::new(3, 4)));
    assert_eq!(symbols, ["s", "NUM", "error"]);
}

#[test]
fn flex_navigation_and_cross_file_tokens() {
    let mut state = State::default();
    let lexer = "%x STR\nD [0-9]\n%%\n{D}+ { return NUMBER; }\n\\\" { BEGIN(STR); }\n<STR>\\\" { BEGIN(INITIAL); }\n";
    assert!(open(&mut state, "calc.l", "flex", lexer).is_empty());
    open(&mut state, "calc.y", "bison", PARSER);
    let u = uri("calc.l");

    let Some(GotoDefinitionResponse::Scalar(loc)) = state.definition(&u, pos(lexer, "{D}", 0, 1))
    else {
        panic!()
    };
    assert_eq!(loc.range.start, Position::new(1, 0));

    // `return NUMBER;` jumps into the grammar.
    let Some(GotoDefinitionResponse::Scalar(loc)) =
        state.definition(&u, pos(lexer, "NUMBER;", 0, 0))
    else {
        panic!()
    };
    assert_eq!(loc.uri, uri("calc.y"));
    assert_eq!(loc.range.start, pos(PARSER, "NUMBER", 0, 0));
    let hover = hover_text(state.hover(&u, pos(lexer, "NUMBER;", 0, 1)).unwrap());
    assert!(hover.contains("%token <num> NUMBER"));

    let hl = state.highlights(&u, pos(lexer, "STR", 0, 0)).unwrap();
    assert_eq!(hl.len(), 3);
    assert!(
        hover_text(state.hover(&u, pos(lexer, "INITIAL", 0, 0)).unwrap())
            .contains("default start condition")
    );
    assert!(state
        .prepare_rename(&u, pos(lexer, "INITIAL", 0, 0))
        .is_none());
}

#[test]
fn flex_completion() {
    let mut state = State::default();
    let text = "%x STR\n%opt\n%option noyy\nDIGIT [0-9]\n%%\n{DI\n<ST\n. { BEGIN(S\n. { return I\n";
    open(&mut state, "calc.l", "flex", text);
    open(&mut state, "calc.y", "bison", PARSER);
    let u = uri("calc.l");
    let at_end = |line: u32| {
        let len = text.lines().nth(line as usize).unwrap().len() as u32;
        Position::new(line, len)
    };
    assert!(labels(state.completion(&u, at_end(1))).contains(&"%option".to_string()));
    assert!(labels(state.completion(&u, at_end(2))).contains(&"noyywrap".to_string()));
    assert_eq!(labels(state.completion(&u, at_end(5))), ["DIGIT"]);
    assert_eq!(labels(state.completion(&u, at_end(6))), ["STR", "INITIAL"]);
    assert_eq!(labels(state.completion(&u, at_end(7))), ["STR", "INITIAL"]);
    let tokens = labels(state.completion(&u, at_end(8)));
    assert!(tokens.contains(&"NUMBER".to_string()) && tokens.contains(&"SEMICOLON".to_string()));
}

#[test]
fn flex_uses_grammar_files_next_to_the_scanner() {
    let dir = std::env::temp_dir().join(format!("flex-bison-lsp-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("calc.y"), PARSER).unwrap();
    let scanner = Url::from_file_path(dir.join("calc.l")).unwrap();
    let text = "%%\n. { return NUMBER; }\n";

    let mut state = State::default();
    // An unrelated open grammar must not hide the one on disk.
    open(
        &mut state,
        "other.y",
        "bison",
        "%token OTHER\n%%\ns: OTHER ;\n",
    );
    state.handle_notification(Notification::new(
        "textDocument/didOpen".into(),
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem::new(scanner.clone(), "flex".into(), 1, text.into()),
        },
    ));
    let end = Position::new(1, text.lines().nth(1).unwrap().find(';').unwrap() as u32);
    let tokens = labels(state.completion(&scanner, end));
    std::fs::remove_dir_all(&dir).unwrap();

    assert!(tokens.contains(&"OTHER".to_string()), "{tokens:?}");
    assert!(tokens.contains(&"NUMBER".to_string()), "{tokens:?}");
}

#[test]
fn sample_lexer_is_clean() {
    let mut state = State::default();
    assert!(open(&mut state, "calc.l", "flex", LEXER).is_empty());
}

fn send(stdin: &mut impl Write, msg: Value) {
    let body = msg.to_string();
    write!(stdin, "Content-Length: {}\r\n\r\n{body}", body.len()).unwrap();
    stdin.flush().unwrap();
}

fn recv(stdout: &mut impl BufRead) -> Value {
    let mut len = 0;
    loop {
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(n) = line.strip_prefix("Content-Length: ") {
            len = n.parse().unwrap();
        }
    }
    let mut body = vec![0; len];
    stdout.read_exact(&mut body).unwrap();
    serde_json::from_slice(&body).unwrap()
}

#[test]
fn speaks_lsp_over_stdio() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_flex-bison-lsp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    send(
        &mut stdin,
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"capabilities": {}}}),
    );
    let init = recv(&mut stdout);
    assert_eq!(init["result"]["capabilities"]["definitionProvider"], true);
    send(
        &mut stdin,
        json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}),
    );
    send(
        &mut stdin,
        json!({"jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {
            "textDocument": {"uri": "file:///x/a.y", "languageId": "bison", "version": 1, "text": "%%\ns: X ;\n"}
        }}),
    );
    let diags = recv(&mut stdout);
    assert_eq!(diags["method"], "textDocument/publishDiagnostics");
    assert_eq!(
        diags["params"]["diagnostics"][0]["range"]["start"],
        json!({"line": 1, "character": 3})
    );

    send(
        &mut stdin,
        json!({"jsonrpc": "2.0", "id": 2, "method": "shutdown"}),
    );
    assert_eq!(recv(&mut stdout)["id"], 2);
    send(&mut stdin, json!({"jsonrpc": "2.0", "method": "exit"}));
    assert!(child.wait().unwrap().success());
}
