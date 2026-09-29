//! Short reference docs for directives and options, used by hover and
//! completion.

pub const BISON_DIRECTIVES: &[(&str, &str)] = &[
    ("%code", "`%code [qualifier] {code}` — insert code into the parser. Qualifiers: `requires`, `provides`, `top`."),
    ("%debug", "Enable parser tracing (`yydebug`). Same as `%define parse.trace`."),
    ("%default-prec", "Assign `%prec` of the last terminal to rules that lack one (the default)."),
    ("%define", "`%define variable [value]` — set a Bison configuration variable, e.g. `%define api.pure full`."),
    ("%defines", "Write a header file with token definitions (deprecated spelling of `%header`)."),
    ("%destructor", "`%destructor {code} symbols...` — code run to discard a semantic value."),
    ("%dprec", "`%dprec N` — GLR: rule priority when resolving ambiguities."),
    ("%empty", "Marks an alternative that matches the empty string."),
    ("%error-verbose", "Produce detailed syntax error messages (deprecated: use `%define parse.error verbose`)."),
    ("%expect", "`%expect N` — the number of shift/reduce conflicts expected."),
    ("%expect-rr", "`%expect-rr N` — the number of reduce/reduce conflicts expected."),
    ("%file-prefix", "`%file-prefix \"prefix\"` — prefix for all output file names."),
    ("%glr-parser", "Generate a GLR parser instead of LALR(1)."),
    ("%header", "Write a header file with token definitions and `YYSTYPE`."),
    ("%initial-action", "`%initial-action {code}` — code run before parsing starts."),
    ("%language", "`%language \"lang\"` — output language: C, C++, D, or Java."),
    ("%left", "`%left [<type>] symbols...` — declare left-associative tokens. Later lines bind tighter."),
    ("%lex-param", "`%lex-param {argument}` — extra argument passed to `yylex`."),
    ("%locations", "Enable location tracking (`@$`, `@1`, `YYLTYPE`)."),
    ("%merge", "`%merge <function>` — GLR: merge ambiguous parses with a function."),
    ("%name-prefix", "`%name-prefix \"prefix\"` — rename external symbols (deprecated: use `%define api.prefix`)."),
    ("%no-default-prec", "Do not assign precedence to rules that lack `%prec`."),
    ("%no-lines", "Do not emit `#line` directives."),
    ("%nonassoc", "`%nonassoc [<type>] symbols...` — declare non-associative tokens (`a < b < c` is an error)."),
    ("%nterm", "`%nterm [<type>] symbols...` — declare nonterminals."),
    ("%output", "`%output \"file\"` — name of the generated parser file."),
    ("%param", "`%param {argument}` — extra argument passed to both `yylex` and `yyparse`."),
    ("%parse-param", "`%parse-param {argument}` — extra argument passed to `yyparse`."),
    ("%prec", "`%prec TOKEN` — give this rule the precedence of `TOKEN`."),
    ("%precedence", "`%precedence [<type>] symbols...` — declare precedence without associativity."),
    ("%printer", "`%printer {code} symbols...` — code to print a semantic value when tracing."),
    ("%pure-parser", "Generate a reentrant parser (deprecated: use `%define api.pure`)."),
    ("%require", "`%require \"version\"` — minimum Bison version."),
    ("%right", "`%right [<type>] symbols...` — declare right-associative tokens."),
    ("%skeleton", "`%skeleton \"file\"` — use a custom parser skeleton."),
    ("%start", "`%start symbol` — the start symbol (defaults to the first rule's left-hand side)."),
    ("%token", "`%token [<type>] NAME [number] [\"alias\"]...` — declare terminal symbols."),
    ("%token-table", "Generate the `yytname` table of token names."),
    ("%type", "`%type <type> symbols...` — declare the semantic value type of symbols."),
    ("%union", "`%union {members}` — declare the possible semantic value types (`YYSTYPE`)."),
    ("%verbose", "Write a `.output` file describing the automaton and its conflicts."),
    ("%yacc", "Emulate POSIX Yacc (output file names and behavior)."),
];

pub const FLEX_DIRECTIVES: &[(&str, &str)] = &[
    ("%option", "`%option name ...` — set scanner options, e.g. `%option noyywrap yylineno`."),
    ("%x", "`%x NAME ...` — declare exclusive start conditions. Only rules prefixed with `<NAME>` are active in that state."),
    ("%s", "`%s NAME ...` — declare inclusive start conditions. Rules without a `<...>` prefix stay active too."),
    ("%top", "`%top{ code }` — code placed at the very top of the generated file, before flex's own definitions."),
    ("%array", "Declare `yytext` as a `char` array (AT&T lex compatible)."),
    ("%pointer", "Declare `yytext` as a `char *` (the default)."),
];

pub const FLEX_OPTIONS: &[(&str, &str)] = &[
    ("7bit", "Generate a 7-bit scanner."),
    ("8bit", "Generate an 8-bit scanner (the default)."),
    ("align", "Trade memory for speed by aligning tables."),
    ("always-interactive", "Treat input as interactive (read one character at a time)."),
    ("array", "Declare `yytext` as an array."),
    ("backup", "Write backing-up information to `lex.backup`."),
    ("batch", "Generate a batch (non-interactive) scanner."),
    ("bison-bridge", "Generate a scanner for a pure Bison parser: `yylex` takes `YYSTYPE *yylval_param`."),
    ("bison-locations", "Like `bison-bridge`, with a `YYLTYPE *yylloc_param` argument."),
    ("c++", "Generate a C++ scanner class."),
    ("case-insensitive", "Ignore case in patterns."),
    ("case-sensitive", "Match case in patterns (the default)."),
    ("caseful", "Match case in patterns (the default)."),
    ("caseless", "Ignore case in patterns."),
    ("debug", "Enable debugging output (`yy_flex_debug`)."),
    ("default", "Generate the default rule (echo unmatched input). `nodefault` makes unmatched input an error."),
    ("ecs", "Construct equivalence classes (the default)."),
    ("extra-type", "`extra-type=\"type\"` — type of `yyextra` in reentrant scanners."),
    ("fast", "Use the fast table representation."),
    ("full", "Use full, uncompressed tables."),
    ("header-file", "`header-file=\"file\"` — also write a header file."),
    ("input", "Generate `input()`. `noinput` silences unused-function warnings."),
    ("interactive", "Generate an interactive scanner."),
    ("lex-compat", "Maximal compatibility with AT&T lex."),
    ("line", "Emit `#line` directives (the default)."),
    ("main", "Provide a default `main()` that calls `yylex()`. Implies `noyywrap`."),
    ("meta-ecs", "Construct meta-equivalence classes."),
    ("never-interactive", "Never treat input as interactive."),
    ("outfile", "`outfile=\"file\"` — name of the generated scanner."),
    ("perf-report", "Report features that slow the scanner down."),
    ("pointer", "Declare `yytext` as a pointer (the default)."),
    ("posix-compat", "Maximal compatibility with POSIX lex."),
    ("prefix", "`prefix=\"name\"` — replace the `yy` prefix of generated names."),
    ("read", "Use `read()` instead of `fread()` for input."),
    ("reentrant", "Generate a reentrant scanner with an explicit `yyscan_t` state."),
    ("reject", "Enable `REJECT`."),
    ("stack", "Enable the start condition stack (`yy_push_state`, `yy_pop_state`)."),
    ("stdinit", "Initialize `yyin`/`yyout` to `stdin`/`stdout`."),
    ("stdout", "Write the scanner to standard output."),
    ("tables-file", "`tables-file=\"file\"` — write serialized tables."),
    ("tables-verify", "Verify serialized tables (debugging)."),
    ("trace", "Trace flex's own progress."),
    ("unistd", "Include `<unistd.h>`. `nounistd` omits it (useful on Windows)."),
    ("unput", "Generate `unput()`. `nounput` silences unused-function warnings."),
    ("verbose", "Print a summary of statistics."),
    ("warn", "Enable warnings (the default)."),
    ("yyalloc", "Generate `yyalloc`. Use `noyyalloc` to provide your own."),
    ("yyclass", "`yyclass=\"name\"` — C++: the `yyFlexLexer` subclass implementing `yylex`."),
    ("yyfree", "Generate `yyfree`. Use `noyyfree` to provide your own."),
    ("yyget_debug", "Generate the `yyget_debug` accessor."),
    ("yyget_extra", "Generate the `yyget_extra` accessor."),
    ("yyget_in", "Generate the `yyget_in` accessor."),
    ("yyget_leng", "Generate the `yyget_leng` accessor."),
    ("yyget_lineno", "Generate the `yyget_lineno` accessor."),
    ("yyget_lloc", "Generate the `yyget_lloc` accessor."),
    ("yyget_lval", "Generate the `yyget_lval` accessor."),
    ("yyget_out", "Generate the `yyget_out` accessor."),
    ("yyget_text", "Generate the `yyget_text` accessor."),
    ("yylineno", "Maintain the current line number in `yylineno`."),
    ("yymore", "Enable `yymore()`."),
    ("yy_pop_state", "Generate `yy_pop_state`."),
    ("yy_push_state", "Generate `yy_push_state`."),
    ("yy_scan_buffer", "Generate `yy_scan_buffer`."),
    ("yy_scan_bytes", "Generate `yy_scan_bytes`."),
    ("yy_scan_string", "Generate `yy_scan_string`."),
    ("yy_top_state", "Generate `yy_top_state`."),
    ("yyrealloc", "Generate `yyrealloc`. Use `noyyrealloc` to provide your own."),
    ("yyset_debug", "Generate the `yyset_debug` accessor."),
    ("yyset_extra", "Generate the `yyset_extra` accessor."),
    ("yyset_in", "Generate the `yyset_in` accessor."),
    ("yyset_lineno", "Generate the `yyset_lineno` accessor."),
    ("yyset_lloc", "Generate the `yyset_lloc` accessor."),
    ("yyset_lval", "Generate the `yyset_lval` accessor."),
    ("yyset_out", "Generate the `yyset_out` accessor."),
    ("yywrap", "Call `yywrap()` at end of input. `noyywrap` assumes there is no more input, so you need not define `yywrap`."),
];

fn lookup(table: &[(&str, &'static str)], name: &str) -> Option<&'static str> {
    table.iter().find(|(n, _)| *n == name).map(|(_, d)| *d)
}

pub fn bison_directive(name: &str) -> Option<&'static str> {
    lookup(BISON_DIRECTIVES, name)
}

pub fn flex_directive(name: &str) -> Option<&'static str> {
    lookup(FLEX_DIRECTIVES, &name.to_ascii_lowercase())
}

pub fn flex_option(name: &str) -> Option<&'static str> {
    lookup(FLEX_OPTIONS, name)
}

/// Docs for a key recorded in `Analysis::directives`.
pub fn for_key(key: &str) -> Option<String> {
    if let Some(option) = key.strip_prefix("option:") {
        return flex_option(option).map(|d| format!("`%option {option}`\n\n{d}"));
    }
    bison_directive(key)
        .or_else(|| flex_directive(key))
        .map(str::to_string)
}
