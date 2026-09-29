%{
#include <stdlib.h>
#include <string.h>
#include "calc.tab.h"
%}

%option noyywrap yylineno
%x COMMENT

DIGIT    [0-9]
LETTER   [A-Za-z_]

%%
"/*"                    { BEGIN(COMMENT); }
<COMMENT>"*/"           { BEGIN(INITIAL); }
<COMMENT>\n             ;
<COMMENT>.              ;
<COMMENT><<EOF>>        {
    fprintf(stderr, "unterminated comment\n");
    return 0;
}

"print"                 { return PRINT; }
{DIGIT}+                { yylval.num = atoi(yytext); return NUMBER; }
{LETTER}({LETTER}|{DIGIT})* {
                          yylval.name = strdup(yytext);
                          return IDENT;
                        }

"+"                     { return PLUS; }
"-"                     { return MINUS; }
"*"                     { return TIMES; }
"/"                     { return DIVIDE; }
"("                     { return LPAREN; }
")"                     { return RPAREN; }
";"                     { return SEMICOLON; }
"="                     |
","                     { return yytext[0]; }
[ \t\n]+                ;
.                       { fprintf(stderr, "unexpected '%s'\n", yytext); }
%%
