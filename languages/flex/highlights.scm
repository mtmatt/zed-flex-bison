(comment) @comment

(directive (identifier) @keyword)
(directive (value) @constant)

(definition (identifier) @type)

(condition (identifier) @constant)
(condition ["<" ">"] @punctuation.bracket)
(condition "," @punctuation.delimiter)
(condition "*" @string.special)

(pattern) @string.regex

(pattern ["(" ")"] @punctuation.bracket)
(pattern ["+" "*" "?" "|" "/"] @operator)
(pattern ["^" "$"] @string.escape)

(bracketed ["[" "]"] @punctuation.bracket)
(bracketed ["^" "-"] @operator)
(character_class ["[:" ":]"] @punctuation.bracket)
(character_class (identifier) @constant.builtin)

(expansion ["{" "}"] @punctuation.bracket)
(expansion (identifier) @type)

(escaped) @string.escape

(quantifier (number) @number)
(quantifier "," @punctuation.delimiter)

(string) @string
(eof) @constant.builtin

(action "|" @punctuation.special)

[
  "%top{"
  "%{"
  "%}"
] @punctuation.special

"%%" @punctuation.special
