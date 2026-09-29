(comment) @comment

[
  (declaration_name)
  (directive)
] @keyword

(grammar_rule_declaration) @function

; By convention tokens are UPPER_CASE and nonterminals lower_case.
((grammar_rule_identifier) @constant
  (#match? @constant "^[A-Z_][A-Z0-9_]*$"))
((grammar_rule_identifier) @variable
  (#not-match? @variable "^[A-Z_][A-Z0-9_]*$"))

(type) @type
(type_tag ["<" ">"] @punctuation.bracket)

(string_literal) @string
(char_literal) @string.special
(number_literal) @number

(decl_language
  language: (string_literal) @string.special)

"%%" @punctuation.special
(prologue ["%{" "%}"] @punctuation.special)
(code_block ["{" "}"] @punctuation.bracket)

[":" "|" ";"] @punctuation.delimiter
