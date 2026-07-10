# Anubis Grammar (Minimum Core — Gate 2/3)

Approximate EBNF for an early slice. **This grammar is intentionally partial and now under-describes the
language** — the authoritative, current syntax reference is [`../../LANGUAGE.md`](../../LANGUAGE.md)
(structs, enums, traits/impls, generics, closures, `match`, `for`/`while`/`loop`, `Option`/`Result` + `?`,
block comments, etc. are all real and specified there). This file is kept for the fixture/runner history.

```ebnf
program     = { item } ;

item        = fn_item | import_item | module_item ;

fn_item     = [ "@" ident ] "fn" ident "(" [ params ] ")" block ;

params      = param { "," param } ;
param       = ident ":" type ;

block       = "{" { stmt } "}" ;

stmt        = let_stmt
            | if_stmt
            | while_stmt          (* REAL — also for/loop/break/continue; see LANGUAGE.md *)
            | return_stmt
            | expr_stmt
            | research_block
            | exploit_block
            | hybrid_block
            | spec_block ;

let_stmt    = "let" ident [ ":" type ] "=" expr ";" ;

if_stmt     = "if" expr block [ "else" block ] ;

expr_stmt   = expr ";" ;

research_block = "research" [ "{" intent "}" ] block ;
exploit_block  = "exploit"  [ "{" intent "}" ] block ;
hybrid_block   = "hybrid" "{" [ "gpu" "(" "metal" ")" block ]
                           [ "cpu" block ]
                           [ "prove" "(" "risc0" ")" block ] "}" ;
spec_block     = "spec" "{" "forall" ident "." expr "}" ;

expr        = primary { binop primary } ;
primary     = ident
            | number
            | string
            | "true" | "false"
            | "(" expr ")"
            | call
            | "symbolic" "(" string ")"
            | "taint_source" "(" string ")"
            | "declassify" "(" expr [ "," string [ "," string ] ] ")"
            | "assume" "(" expr ")"
            | "assert" "(" expr ")" ;

call        = ident "(" [ expr { "," expr } ] ")" ;

binop       = "+" | "-" | "*" | "==" | "!=" | "<" | "<=" | ">" | ">=" | "&" ;

type        = "bool" | "u8" | "u16" | "u32" | "u64" | "string"
            | "tainted" "<" type ">"
            | ident ;   (* struct name *)

struct_decl = "struct" ident "{" { ident ":" type ";" } "}" ;

import_item = "import" string ";" ;
module_item = "module" ident block ;
```

**Notes for this slice:**
- `while_stmt`, full struct decl in grammar above are aspirational for fixtures; actual parser may accept subset.
- Attributes parsed as leading `@ident` or via mode inference from blocks.
- Error productions produce diagnostics, never panic for the fixture set.
