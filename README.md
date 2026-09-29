# Flex & Bison for Zed

Zed support for [Flex](https://github.com/westes/flex) scanners (`.l`, `.ll`,
`.lex`, `.flex`) and [Bison](https://www.gnu.org/software/bison/) grammars
(`.y`, `.yy`, `.ypp`, `.bison`):

- Syntax highlighting via tree-sitter, with C highlighting inside actions,
  `%{ %}` blocks and the user-code section.
- `flex-bison-lsp`, a language server in [`server/`](server).

## Language server features

| Feature                      | Bison                                                                                                                                  | Flex                                                                                                                                            |
| ---------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| Diagnostics                  | undefined symbols, rules for tokens, unreachable or non-productive nonterminals, missing `%%`, unknown directives, unterminated blocks | undefined `{NAME}`, undeclared start conditions, unused definitions and conditions, unknown `%option`s, unterminated actions, scopes and blocks |
| Go to definition             | tokens and nonterminals                                                                                                                | `{NAME}` definitions, start conditions (`<SC>`, `BEGIN(SC)`), and tokens in actions (`return NUM;`) jump into the `.y` file                     |
| Find references / highlights | yes (including `"alias"` strings)                                                                                                      | yes                                                                                                                                             |
| Hover                        | token declarations, nonterminal rules, directive docs                                                                                  | definition patterns, start conditions, `%option` docs, Bison tokens                                                                             |
| Completion                   | directives after `%`, symbols                                                                                                          | directives, options, definitions after `{`, conditions after `<` or `BEGIN(`, Bison tokens in actions                                           |
| Rename                       | tokens and nonterminals                                                                                                                | definitions and start conditions                                                                                                                |
| Outline                      | rules and tokens                                                                                                                       | definitions and start conditions                                                                                                                |

Cross-file lookups from a scanner search open grammar files first, then `.y`
files in the scanner's directory.

## Installation

1. Install the language server so it is on your `PATH`:

   ```sh
   cargo install --path server
   ```

2. In Zed, run `zed: install dev extension` and choose this directory.

To use a server binary somewhere else, set its path in Zed's `settings.json`:

```json
{
  "lsp": {
    "flex-bison-lsp": {
      "binary": { "path": "/path/to/flex-bison-lsp" }
    }
  }
}
```

## Known limitations

These come from the upstream tree-sitter grammars and only affect syntax
highlighting (the language server has its own parser):

- Bison: `%prec` inside a rule is not recognized.
- Flex: a `%option` line with several options, e.g. `%option noyywrap yylineno`,
  is only partly recognized. One option per line highlights correctly.

## Development

```sh
cd server && cargo test        # server unit and integration tests
cargo build --target wasm32-wasip2 --release   # the Zed extension itself
```

Grammars: [tree-sitter-bison](https://github.com/fanwenlin/tree-sitter-bison) (a WASI-compatible fork of [btuin2/tree-sitter-bison](https://gitlab.com/btuin2/tree-sitter-bison))
and [tree-sitter-flex](https://github.com/m4rch3n1ng/tree-sitter-flex).
