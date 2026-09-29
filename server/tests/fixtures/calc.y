/* A small calculator language. `ASSIGN` and `COMMA` are deliberately left
   undeclared so the tests can check the "undefined symbol" diagnostics. */
%{
#include <stdio.h>
int yylex(void);
void yyerror(const char *s);
%}

%union {
    int num;
    char *name;
}

%token <num> NUMBER
%token <name> IDENT
%token PRINT LPAREN RPAREN SEMICOLON

%left PLUS MINUS
%left TIMES DIVIDE
%right UMINUS

%type <num> expr

%%
program : stmt_list ;

stmt_list : stmt_list stmt
          | stmt
          ;

stmt : PRINT expr SEMICOLON         { printf("%d\n", $2); }
     | IDENT ASSIGN expr SEMICOLON
     ;

args : args COMMA expr
     | expr
     ;

expr : expr PLUS expr               { $$ = $1 + $3; }
     | expr MINUS expr              { $$ = $1 - $3; }
     | expr TIMES expr              { $$ = $1 * $3; }
     | expr DIVIDE expr             { $$ = $3 ? $1 / $3 : 0; }
     | MINUS expr %prec UMINUS      { $$ = -$2; }
     | LPAREN expr RPAREN           { $$ = $2; }
     | IDENT LPAREN args RPAREN     { $$ = 0; }
     | NUMBER
     ;
%%

void yyerror(const char *s) {
    fprintf(stderr, "%s\n", s);
}
