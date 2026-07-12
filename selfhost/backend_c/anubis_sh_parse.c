/* anubis_sh_parse.c — C-native Anubis-SH lexer + parser.
 *
 * Phase A / Diverse Double-Compiling capstone: closes the "no non-rustc Anubis
 * parser exists" residual (docs/language/SELFHOST.md). This is a faithful,
 * hand-written C port of sh_lex + sh_parse + the jast/jexpr/jstmt emitters in
 * selfhost/src/anubis_sh.anb. Compiled with a non-LLVM C compiler (gcc), it
 * derives the anubis_sh AST payload from SOURCE TEXT with ZERO rustc involvement.
 *
 * The oracle that validates this port is byte-identity: for anubis_sh.anb and the
 * corpus, `anubis_sh_parse parse <f>` must equal `anubis run anubis_sh.anb -- parse <f>`
 * byte-for-byte (and likewise for `lex`). If they agree, the C port reproduces the
 * reference parser exactly — the same discipline that validated the interpreter port.
 *
 * Usage:  anubis_sh_parse (parse|lex) <file.anb>
 * Output: compact JSON AST (parse) or token array (lex) + trailing newline, to stdout.
 * On parse error: "PARSE_ERROR: <msg> at pos <p> tok <kind>:<text> @<start>\n", exit 1.
 *
 * ASCII-only, like the self-host source. Positions are BYTE offsets (the reference
 * interpreter byte-indexes char_at/substr/ord), which coincides with chars for ASCII.
 *
 * Memory: nodes are malloc'd and never freed — the process is short-lived and exits.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <setjmp.h>

/* ------------------------------------------------------------------ util --- */
static void oom(void) { fputs("anubis_sh_parse: out of memory\n", stderr); exit(2); }
static void *xmalloc(size_t n) { void *p = malloc(n ? n : 1); if (!p) oom(); return p; }
static void *xrealloc(void *p, size_t n) { void *q = realloc(p, n ? n : 1); if (!q) oom(); return q; }
static char *xstrdup(const char *s) { size_t n = strlen(s) + 1; char *p = xmalloc(n); memcpy(p, s, n); return p; }

/* Growable byte buffer (output) and generic pointer vector. */
typedef struct { char *d; size_t n, cap; } Buf;
static void buf_reserve(Buf *b, size_t extra) {
    if (b->n + extra > b->cap) {
        size_t c = b->cap ? b->cap : 256;
        while (c < b->n + extra) c *= 2;
        b->d = xrealloc(b->d, c);
        b->cap = c;
    }
}
static void buf_putc(Buf *b, char c) { buf_reserve(b, 1); b->d[b->n++] = c; }
static void buf_write(Buf *b, const char *s, size_t len) { buf_reserve(b, len); memcpy(b->d + b->n, s, len); b->n += len; }
static void buf_puts(Buf *b, const char *s) { buf_write(b, s, strlen(s)); }

typedef struct { void **d; int n, cap; } Vec;
static void vec_push(Vec *v, void *p) {
    if (v->n == v->cap) { v->cap = v->cap ? v->cap * 2 : 4; v->d = xrealloc(v->d, (size_t)v->cap * sizeof(void *)); }
    v->d[v->n++] = p;
}

/* ----------------------------------------------------------------- lexer --- */
typedef struct { const char *kind; char *text; long start; long end; } Token;

static int is_ws(int o)    { return o == 32 || o == 9 || o == 10 || o == 13; }
static int is_alpha(int o) { return (o >= 65 && o <= 90) || (o >= 97 && o <= 122) || o == 95; }
static int is_digit(int o) { return o >= 48 && o <= 57; }
static int is_alnum(int o) { return is_alpha(o) || is_digit(o); }

static int is_kw(const char *t) {
    return !strcmp(t, "fn") || !strcmp(t, "let") || !strcmp(t, "mut") || !strcmp(t, "if") ||
           !strcmp(t, "else") || !strcmp(t, "while") || !strcmp(t, "for") || !strcmp(t, "in") ||
           !strcmp(t, "return") || !strcmp(t, "pub") || !strcmp(t, "true") || !strcmp(t, "false") ||
           !strcmp(t, "requires") || !strcmp(t, "ensures") || !strcmp(t, "uses");
}

/* substr(src, start, len) -> fresh NUL-terminated copy. */
static char *slice(const char *src, long start, long len) {
    char *p = xmalloc((size_t)len + 1);
    memcpy(p, src + start, (size_t)len);
    p[len] = 0;
    return p;
}

static Token *mk_tok(Vec *toks) {
    Token *t = xmalloc(sizeof(Token));
    vec_push(toks, t);
    return t;
}
static void push_tok(Vec *toks, const char *kind, char *text, long start, long end) {
    Token *t = mk_tok(toks);
    t->kind = kind; t->text = text; t->start = start; t->end = end;
}

/* Faithful port of sh_lex. Returns a Vec of Token*. */
static Vec sh_lex(const char *src) {
    Vec toks = {0};
    long i = 0, n = (long)strlen(src);
    while (i < n) {
        unsigned char c = (unsigned char)src[i];
        int o = c;
        if (is_ws(o)) { i++; continue; }
        if (c == '/' && i + 1 < n && src[i + 1] == '/') {
            i += 2;
            while (i < n) { if ((unsigned char)src[i] == 10) break; i++; }
            continue;
        }
        if (c == '/' && i + 1 < n && src[i + 1] == '*') {
            i += 2;
            long depth = 1;
            while (i < n) {
                if (depth == 0) break;
                if (src[i] == '/' && i + 1 < n && src[i + 1] == '*') { depth++; i += 2; }
                else if (src[i] == '*' && i + 1 < n && src[i + 1] == '/') { depth--; i += 2; }
                else i++;
            }
            continue;
        }
        long start = i;
        if (is_alpha(o)) {
            i++;
            while (i < n && is_alnum((unsigned char)src[i])) i++;
            char *text = slice(src, start, i - start);
            push_tok(&toks, is_kw(text) ? "Keyword" : "Ident", text, start, i);
            continue;
        }
        if (is_digit(o)) {
            i++;
            while (i < n && is_digit((unsigned char)src[i])) i++;
            push_tok(&toks, "Number", slice(src, start, i - start), start, i);
            continue;
        }
        if (c == '"') {
            i++;
            Buf t = {0};
            while (i < n) {
                if (src[i] == '"') break;
                if (src[i] == '\\' && i + 1 < n) {
                    char nx = src[i + 1];
                    if (nx == 'n') buf_putc(&t, (char)10);
                    else if (nx == 't') buf_putc(&t, (char)9);
                    else buf_putc(&t, nx);
                    i += 2;
                } else {
                    buf_putc(&t, src[i]);
                    i++;
                }
            }
            if (i < n) i++;
            buf_putc(&t, 0);
            push_tok(&toks, "String", t.d ? t.d : xstrdup(""), start, i);
            continue;
        }
        /* operators */
        int advanced = 0;
        if (i + 1 < n) {
            char a = src[i], b = src[i + 1];
            const char *k = NULL;
            if (a == ':' && b == ':') k = "ColonColon";
            else if (a == '.' && b == '.') k = "DotDot";
            else if (a == '&' && b == '&') k = "AmpAmp";
            else if (a == '|' && b == '|') k = "PipePipe";
            else if (a == '=' && b == '=') k = "EqEq";
            else if (a == '!' && b == '=') k = "Ne";
            else if (a == '<' && b == '=') k = "Le";
            else if (a == '>' && b == '=') k = "Ge";
            if (k) {
                char *two = slice(src, i, 2);
                push_tok(&toks, k, two, start, i + 2);
                i += 2; advanced = 1;
            }
        }
        if (!advanced) {
            const char *k;
            switch (c) {
                case '(': k = "LParen"; break;    case ')': k = "RParen"; break;
                case '{': k = "LBrace"; break;    case '}': k = "RBrace"; break;
                case '[': k = "LBracket"; break;  case ']': k = "RBracket"; break;
                case ':': k = "Colon"; break;     case ';': k = "Semi"; break;
                case ',': k = "Comma"; break;     case '.': k = "Dot"; break;
                case '+': k = "Plus"; break;      case '-': k = "Minus"; break;
                case '*': k = "Star"; break;      case '/': k = "Slash"; break;
                case '%': k = "Percent"; break;   case '=': k = "Eq"; break;
                case '<': k = "Lt"; break;        case '>': k = "Gt"; break;
                case '!': k = "Bang"; break;      case '&': k = "Amp"; break;
                case '|': k = "Pipe"; break;      default:  k = "Other"; break;
            }
            push_tok(&toks, k, slice(src, start, 1), start, start + 1);
            i++;
        }
    }
    return toks;
}

/* ------------------------------------------------------------------- AST --- */
typedef enum {
    E_NONE, E_INT, E_STR, E_BOOL, E_VAR, E_CALL, E_BINARY, E_UNARY,
    E_INDEX, E_LIST, E_RANGE, E_MAP, E_ENUMINIT, E_IFEXPR, E_MATCH
} EKind;
typedef enum { S_LET, S_ASSIGN, S_RETURN, S_EXPRSTMT, S_IF, S_WHILE, S_FOR } SKind;

typedef struct Expr Expr;
typedef struct Stmt Stmt;

struct Expr {
    EKind kind;
    char *s;            /* Int/Str value text; Var name */
    int   bval;         /* Bool */
    char *callee; Vec args;
    char *op; Expr *lhs; Expr *rhs; Expr *operand;
    Expr *base; Expr *index;
    Vec elements;
    Expr *rstart; Expr *rend;
    Vec keys; Vec vals; /* keys: char*, vals: Expr* */
    char *ei_ty, *ei_variant, *ei_shape; Vec ei_tuple, ei_fnames, ei_fexprs;
    Expr *if_cond; Vec if_then, if_else; /* Stmt* */
    Expr *m_scrut; Vec m_arms; /* Arm* */
};

typedef struct { const char *pk; char *variant; char *shape; Vec fnames; Vec binds; } Pat;
typedef struct { Pat *pat; Expr *body; } Arm;

struct Stmt {
    SKind kind;
    char *let_name, *let_ty; Expr *let_init;
    Expr *as_target, *as_value;
    Expr *ret_value;
    Expr *es_expr;
    Expr *if_cond; Vec if_then, if_else; int has_else;
    Expr *wh_cond; Vec wh_body;
    char *for_var; Expr *for_iter; Vec for_body;
};

typedef struct { char *name, *ty; } Param;
typedef struct { char *name, *shape; } Variant;
typedef struct {
    int is_enum;
    char *name; int is_pub; Vec params; char *ret;
    Vec requires_, ensures_, effects; Vec body;   /* effects: char* */
    char *enum_name; Vec variants;                /* Variant* */
} Item;

static Expr *new_expr(EKind k) { Expr *e = xmalloc(sizeof(Expr)); memset(e, 0, sizeof(*e)); e->kind = k; return e; }
static Stmt *new_stmt(SKind k) { Stmt *s = xmalloc(sizeof(Stmt)); memset(s, 0, sizeof(*s)); s->kind = k; return s; }

/* --------------------------------------------------------------- parser --- */
static Token   *g_toks;
static int      g_ntok;
static jmp_buf  g_err_jmp;
static char    *g_err_msg;   /* perr message */
static int      g_err_pos;   /* perr pos */
static Token    g_eof = { "Eof", (char *)"", 0, 0 };

static void perr(const char *msg, int pos) { g_err_msg = xstrdup(msg); g_err_pos = pos; longjmp(g_err_jmp, 1); }
static int  at_end(int pos) { return pos >= g_ntok; }
static Token *peek(int pos) { return at_end(pos) ? &g_eof : &g_toks[pos]; }
static int  kind_is(int pos, const char *k) { return !strcmp(peek(pos)->kind, k); }
static int  text_is(int pos, const char *t) { return !strcmp(peek(pos)->text, t); }

/* Parser results thread `pos` by returning the node and writing *out_pos. */
static Expr *parse_expr(int pos, int min_prec, int *out_pos);
static Expr *parse_prefix(int pos, int *out_pos);
static Expr *parse_if_expr(int pos, int *out_pos);
static Expr *parse_match_expr(int pos, int *out_pos);
static Expr *parse_enum_init(int pos, const char *ty, int *out_pos);
static Pat  *parse_pattern(int pos, int *out_pos);
static Stmt *parse_stmt(int pos, int *out_pos);
static Vec   parse_block(int pos, int *out_pos);  /* returns Vec of Stmt* */

static int prec_of(const char *op) {
    if (!strcmp(op, "||")) return 1;
    if (!strcmp(op, "&&")) return 2;
    if (!strcmp(op, "==") || !strcmp(op, "!=")) return 3;
    if (!strcmp(op, "<") || !strcmp(op, ">") || !strcmp(op, "<=") || !strcmp(op, ">=")) return 4;
    if (!strcmp(op, "+") || !strcmp(op, "-")) return 5;
    if (!strcmp(op, "*") || !strcmp(op, "/") || !strcmp(op, "%")) return 6;
    return 0;
}

/* parse_type: returns the type text, advances pos. */
static char *parse_type(int pos, int *out_pos) {
    Token *t = peek(pos);
    if (!strcmp(t->kind, "Ident") || !strcmp(t->kind, "Keyword")) { *out_pos = pos + 1; return t->text; }
    perr("type", pos); return NULL;
}

static Expr *parse_prefix(int pos, int *out_pos) {
    Token *t = peek(pos);
    if (!strcmp(t->text, "if") && !strcmp(t->kind, "Keyword")) return parse_if_expr(pos, out_pos);
    if (!strcmp(t->text, "match") && !strcmp(t->kind, "Ident")) return parse_match_expr(pos, out_pos);
    if (!strcmp(t->kind, "Number")) { Expr *e = new_expr(E_INT); e->s = t->text; *out_pos = pos + 1; return e; }
    if (!strcmp(t->kind, "String")) { Expr *e = new_expr(E_STR); e->s = t->text; *out_pos = pos + 1; return e; }
    if (!strcmp(t->text, "true"))  { Expr *e = new_expr(E_BOOL); e->bval = 1; *out_pos = pos + 1; return e; }
    if (!strcmp(t->text, "false")) { Expr *e = new_expr(E_BOOL); e->bval = 0; *out_pos = pos + 1; return e; }
    if (!strcmp(t->kind, "Ident") || !strcmp(t->kind, "Keyword")) {
        char *name = t->text;
        if (!strcmp(name, "true"))  { Expr *e = new_expr(E_BOOL); e->bval = 1; *out_pos = pos + 1; return e; }
        if (!strcmp(name, "false")) { Expr *e = new_expr(E_BOOL); e->bval = 0; *out_pos = pos + 1; return e; }
        pos = pos + 1;
        if (kind_is(pos, "ColonColon")) return parse_enum_init(pos + 1, name, out_pos);
        if (kind_is(pos, "LParen")) {
            pos = pos + 1;
            Expr *e = new_expr(E_CALL); e->callee = name;
            if (!kind_is(pos, "RParen")) {
                Expr *a0 = parse_expr(pos, 0, &pos); vec_push(&e->args, a0);
                while (kind_is(pos, "Comma")) { pos = pos + 1; Expr *a = parse_expr(pos, 0, &pos); vec_push(&e->args, a); }
            }
            if (!kind_is(pos, "RParen")) perr("expected )", pos);
            *out_pos = pos + 1; return e;
        }
        Expr *e = new_expr(E_VAR); e->s = name; *out_pos = pos; return e;
    }
    if (!strcmp(t->kind, "LParen")) {
        Expr *e = parse_expr(pos + 1, 0, &pos);
        if (!kind_is(pos, "RParen")) perr("expected )", pos);
        *out_pos = pos + 1; return e;
    }
    if (!strcmp(t->kind, "LBracket")) {
        pos = pos + 1;
        Expr *e = new_expr(E_LIST);
        if (!kind_is(pos, "RBracket")) {
            Expr *e0 = parse_expr(pos, 0, &pos); vec_push(&e->elements, e0);
            while (kind_is(pos, "Comma")) { pos = pos + 1; Expr *x = parse_expr(pos, 0, &pos); vec_push(&e->elements, x); }
        }
        if (!kind_is(pos, "RBracket")) perr("expected ]", pos);
        *out_pos = pos + 1; return e;
    }
    if (!strcmp(t->kind, "LBrace")) {
        pos = pos + 1;
        Expr *e = new_expr(E_MAP);
        if (!kind_is(pos, "RBrace")) {
            while (1) {
                Token *kt = peek(pos);
                if (strcmp(kt->kind, "String") && strcmp(kt->kind, "Ident") && strcmp(kt->kind, "Keyword")) perr("map key", pos);
                pos = pos + 1;
                char *key = kt->text;
                if (!kind_is(pos, "Colon")) perr("expected :", pos);
                pos = pos + 1;
                Expr *v = parse_expr(pos, 0, &pos);
                vec_push(&e->keys, key); vec_push(&e->vals, v);
                if (kind_is(pos, "Comma")) pos = pos + 1; else break;
            }
        }
        if (!kind_is(pos, "RBrace")) perr("expected }", pos);
        *out_pos = pos + 1; return e;
    }
    if (!strcmp(t->kind, "Bang") || !strcmp(t->kind, "Minus")) {
        char *op = t->text;
        Expr *inner = parse_prefix(pos + 1, &pos);
        Expr *e = new_expr(E_UNARY); e->op = op; e->operand = inner; *out_pos = pos; return e;
    }
    { /* "bad expr near " + text */
        size_t ln = strlen("bad expr near ") + strlen(t->text) + 1;
        char *m = xmalloc(ln); snprintf(m, ln, "bad expr near %s", t->text);
        g_err_msg = m; g_err_pos = pos; longjmp(g_err_jmp, 1);
    }
    return NULL;
}

static Expr *parse_expr(int pos, int min_prec, int *out_pos) {
    Expr *node = parse_prefix(pos, &pos);
    while (kind_is(pos, "LBracket")) {
        Expr *ix = parse_expr(pos + 1, 0, &pos);
        if (!kind_is(pos, "RBracket")) perr("expected ]", pos);
        pos = pos + 1;
        Expr *idx = new_expr(E_INDEX); idx->base = node; idx->index = ix; node = idx;
    }
    while (1) {
        Token *t = peek(pos);
        int p = prec_of(t->text);
        if (p == 0 || p < min_prec) break;
        char *op = t->text;
        Expr *right = parse_expr(pos + 1, p + 1, &pos);
        Expr *b = new_expr(E_BINARY); b->op = op; b->lhs = node; b->rhs = right; node = b;
    }
    *out_pos = pos; return node;
}

static Expr *parse_enum_init(int pos, const char *ty, int *out_pos) {
    Token *vt = peek(pos);
    char *variant = vt->text;
    pos = pos + 1;
    Expr *e = new_expr(E_ENUMINIT); e->ei_ty = (char *)ty; e->ei_variant = variant;
    if (kind_is(pos, "LParen")) {
        pos = pos + 1;
        if (!kind_is(pos, "RParen")) {
            Expr *a0 = parse_expr(pos, 0, &pos); vec_push(&e->ei_tuple, a0);
            while (kind_is(pos, "Comma")) { pos = pos + 1; Expr *a = parse_expr(pos, 0, &pos); vec_push(&e->ei_tuple, a); }
        }
        if (!kind_is(pos, "RParen")) perr("expected ) in variant", pos);
        pos = pos + 1; e->ei_shape = "tuple"; *out_pos = pos; return e;
    }
    if (kind_is(pos, "LBrace")) {
        pos = pos + 1;
        if (!kind_is(pos, "RBrace")) {
            while (1) {
                Token *fnt = peek(pos); pos = pos + 1;
                if (!kind_is(pos, "Colon")) perr("expected : in variant", pos);
                pos = pos + 1;
                Expr *v = parse_expr(pos, 0, &pos);
                vec_push(&e->ei_fnames, fnt->text); vec_push(&e->ei_fexprs, v);
                if (kind_is(pos, "Comma")) pos = pos + 1; else break;
            }
        }
        if (!kind_is(pos, "RBrace")) perr("expected } in variant", pos);
        pos = pos + 1; e->ei_shape = "struct"; *out_pos = pos; return e;
    }
    e->ei_shape = "unit"; *out_pos = pos; return e;
}

static Expr *parse_if_expr(int pos, int *out_pos) {
    Expr *cond = parse_expr(pos + 1, 0, &pos);
    Vec th = parse_block(pos, &pos);
    Vec el = {0};
    if (text_is(pos, "else")) {
        pos = pos + 1;
        if (text_is(pos, "if")) {
            Expr *nested = parse_if_expr(pos, &pos);
            Stmt *es = new_stmt(S_EXPRSTMT); es->es_expr = nested;
            vec_push(&el, es);
        } else {
            el = parse_block(pos, &pos);
        }
    }
    Expr *e = new_expr(E_IFEXPR); e->if_cond = cond; e->if_then = th; e->if_else = el;
    *out_pos = pos; return e;
}

static Expr *parse_match_expr(int pos, int *out_pos) {
    Expr *scrut = parse_expr(pos + 1, 0, &pos);
    if (!kind_is(pos, "LBrace")) perr("expected { in match", pos);
    pos = pos + 1;
    Expr *e = new_expr(E_MATCH); e->m_scrut = scrut;
    while (!at_end(pos) && !kind_is(pos, "RBrace")) {
        Pat *pat = parse_pattern(pos, &pos);
        if (!kind_is(pos, "Eq")) perr("expected => in match arm", pos);
        pos = pos + 1;
        if (!kind_is(pos, "Gt")) perr("expected => in match arm", pos);
        pos = pos + 1;
        Expr *body = parse_expr(pos, 0, &pos);
        Arm *arm = xmalloc(sizeof(Arm)); arm->pat = pat; arm->body = body;
        vec_push(&e->m_arms, arm);
        if (kind_is(pos, "Comma")) pos = pos + 1;
    }
    if (!kind_is(pos, "RBrace")) perr("expected } in match", pos);
    pos = pos + 1;
    *out_pos = pos; return e;
}

static Pat *parse_pattern(int pos, int *out_pos) {
    Token *t = peek(pos);
    Pat *p = xmalloc(sizeof(Pat)); memset(p, 0, sizeof(*p));
    if (!strcmp(t->text, "_")) { p->pk = "wild"; *out_pos = pos + 1; return p; }
    pos = pos + 1;
    if (kind_is(pos, "ColonColon")) {
        pos = pos + 1;
        Token *vt = peek(pos); char *variant = vt->text; pos = pos + 1;
        p->pk = "enum"; p->variant = variant;
        if (kind_is(pos, "LParen")) {
            pos = pos + 1;
            if (!kind_is(pos, "RParen")) {
                while (1) {
                    Token *b = peek(pos); pos = pos + 1; vec_push(&p->binds, b->text);
                    if (kind_is(pos, "Comma")) pos = pos + 1; else break;
                }
            }
            if (!kind_is(pos, "RParen")) perr("expected ) in pattern", pos);
            pos = pos + 1; p->shape = "tuple"; *out_pos = pos; return p;
        }
        if (kind_is(pos, "LBrace")) {
            pos = pos + 1;
            if (!kind_is(pos, "RBrace")) {
                while (1) {
                    Token *fnt = peek(pos); pos = pos + 1;
                    if (!kind_is(pos, "Colon")) perr("expected : in pattern", pos);
                    pos = pos + 1;
                    Token *bnt = peek(pos); pos = pos + 1;
                    vec_push(&p->fnames, fnt->text); vec_push(&p->binds, bnt->text);
                    if (kind_is(pos, "Comma")) pos = pos + 1; else break;
                }
            }
            if (!kind_is(pos, "RBrace")) perr("expected } in pattern", pos);
            pos = pos + 1; p->shape = "struct"; *out_pos = pos; return p;
        }
        p->shape = "unit"; *out_pos = pos; return p;
    }
    p->pk = "wild"; *out_pos = pos; return p;
}

static Item *parse_enum(int pos, int *out_pos) {
    pos = pos + 1;
    Token *name_t = peek(pos); char *name = name_t->text; pos = pos + 1;
    if (!kind_is(pos, "LBrace")) perr("expected { in enum", pos);
    pos = pos + 1;
    Item *it = xmalloc(sizeof(Item)); memset(it, 0, sizeof(*it)); it->is_enum = 1; it->enum_name = name;
    while (!at_end(pos) && !kind_is(pos, "RBrace")) {
        Token *vt = peek(pos); char *vname = vt->text; pos = pos + 1;
        Variant *v = xmalloc(sizeof(Variant)); v->name = vname;
        if (kind_is(pos, "LParen")) {
            pos = pos + 1;
            if (!kind_is(pos, "RParen")) {
                while (1) {
                    (void)parse_type(pos, &pos);
                    if (kind_is(pos, "Comma")) pos = pos + 1; else break;
                }
            }
            if (!kind_is(pos, "RParen")) perr("expected ) in enum", pos);
            pos = pos + 1; v->shape = "tuple";
        } else if (kind_is(pos, "LBrace")) {
            pos = pos + 1;
            if (!kind_is(pos, "RBrace")) {
                while (1) {
                    (void)peek(pos); pos = pos + 1;
                    if (!kind_is(pos, "Colon")) perr("expected : in enum", pos);
                    pos = pos + 1;
                    (void)parse_type(pos, &pos);
                    if (kind_is(pos, "Comma")) pos = pos + 1; else break;
                }
            }
            if (!kind_is(pos, "RBrace")) perr("expected } in enum", pos);
            pos = pos + 1; v->shape = "struct";
        } else {
            v->shape = "unit";
        }
        vec_push(&it->variants, v);
        if (kind_is(pos, "Comma")) pos = pos + 1;
    }
    if (!kind_is(pos, "RBrace")) perr("expected } in enum", pos);
    pos = pos + 1;
    *out_pos = pos; return it;
}

static Vec parse_block(int pos, int *out_pos) {
    if (!kind_is(pos, "LBrace")) perr("expected {", pos);
    pos = pos + 1;
    Vec stmts = {0};
    while (!at_end(pos) && !kind_is(pos, "RBrace")) {
        Stmt *st = parse_stmt(pos, &pos);
        vec_push(&stmts, st);
    }
    if (!kind_is(pos, "RBrace")) perr("expected }", pos);
    *out_pos = pos + 1; return stmts;
}

static Stmt *parse_stmt(int pos, int *out_pos) {
    if (text_is(pos, "let")) {
        pos = pos + 1;
        if (text_is(pos, "mut")) pos = pos + 1;
        Token *name_t = peek(pos); pos = pos + 1;
        char *ty = "";
        if (kind_is(pos, "Colon")) ty = parse_type(pos + 1, &pos);
        if (!kind_is(pos, "Eq")) perr("expected =", pos);
        Expr *init = parse_expr(pos + 1, 0, &pos);
        if (kind_is(pos, "Semi")) pos = pos + 1;
        Stmt *s = new_stmt(S_LET); s->let_name = name_t->text; s->let_ty = ty; s->let_init = init;
        *out_pos = pos; return s;
    }
    if (text_is(pos, "break")) {
        pos = pos + 1;
        if (kind_is(pos, "Semi")) pos = pos + 1;
        Expr *call = new_expr(E_CALL); call->callee = "break";
        Stmt *s = new_stmt(S_EXPRSTMT); s->es_expr = call;
        *out_pos = pos; return s;
    }
    if (text_is(pos, "return")) {
        pos = pos + 1;
        Expr *val = new_expr(E_NONE);
        if (!kind_is(pos, "Semi") && !kind_is(pos, "RBrace")) val = parse_expr(pos, 0, &pos);
        if (kind_is(pos, "Semi")) pos = pos + 1;
        Stmt *s = new_stmt(S_RETURN); s->ret_value = val;
        *out_pos = pos; return s;
    }
    if (text_is(pos, "if")) {
        Expr *cond = parse_expr(pos + 1, 0, &pos);
        Vec th = parse_block(pos, &pos);
        Vec el = {0}; int has_else = 0;
        if (text_is(pos, "else")) { el = parse_block(pos + 1, &pos); has_else = 1; }
        Stmt *s = new_stmt(S_IF); s->if_cond = cond; s->if_then = th; s->if_else = el; s->has_else = has_else;
        *out_pos = pos; return s;
    }
    if (text_is(pos, "while")) {
        Expr *cond = parse_expr(pos + 1, 0, &pos);
        Vec body = parse_block(pos, &pos);
        Stmt *s = new_stmt(S_WHILE); s->wh_cond = cond; s->wh_body = body;
        *out_pos = pos; return s;
    }
    if (text_is(pos, "for")) {
        pos = pos + 1;
        Token *var_t = peek(pos); pos = pos + 1;
        if (!text_is(pos, "in")) perr("expected in", pos);
        pos = pos + 1;
        Expr *start = parse_expr(pos, 0, &pos);
        if (kind_is(pos, "DotDot")) {
            Expr *end = parse_expr(pos + 1, 0, &pos);
            Vec body = parse_block(pos, &pos);
            Expr *range = new_expr(E_RANGE); range->rstart = start; range->rend = end;
            Stmt *s = new_stmt(S_FOR); s->for_var = var_t->text; s->for_iter = range; s->for_body = body;
            *out_pos = pos; return s;
        }
        Vec body = parse_block(pos, &pos);
        Stmt *s = new_stmt(S_FOR); s->for_var = var_t->text; s->for_iter = start; s->for_body = body;
        *out_pos = pos; return s;
    }
    Expr *e = parse_expr(pos, 0, &pos);
    if (kind_is(pos, "Eq")) {
        Expr *v = parse_expr(pos + 1, 0, &pos);
        if (kind_is(pos, "Semi")) pos = pos + 1;
        Stmt *s = new_stmt(S_ASSIGN); s->as_target = e; s->as_value = v;
        *out_pos = pos; return s;
    }
    if (kind_is(pos, "Semi")) pos = pos + 1;
    Stmt *s = new_stmt(S_EXPRSTMT); s->es_expr = e;
    *out_pos = pos; return s;
}

static Item *parse_fn(int pos, int *out_pos) {
    Item *it = xmalloc(sizeof(Item)); memset(it, 0, sizeof(*it));
    if (text_is(pos, "pub")) { pos = pos + 1; it->is_pub = 1; }
    if (!text_is(pos, "fn")) perr("expected fn", pos);
    pos = pos + 1;
    Token *name_t = peek(pos); pos = pos + 1;
    if (!kind_is(pos, "LParen")) perr("expected (", pos);
    pos = pos + 1;
    if (!kind_is(pos, "RParen")) {
        while (1) {
            Token *pn = peek(pos); pos = pos + 1;
            char *pty = "";
            if (kind_is(pos, "Colon")) pty = parse_type(pos + 1, &pos);
            Param *p = xmalloc(sizeof(Param)); p->name = pn->text; p->ty = pty;
            vec_push(&it->params, p);
            if (kind_is(pos, "Comma")) pos = pos + 1; else break;
        }
    }
    if (!kind_is(pos, "RParen")) perr("expected )", pos);
    pos = pos + 1;
    char *ret = "";
    if (kind_is(pos, "Minus")) {
        pos = pos + 1;
        if (!kind_is(pos, "Gt")) perr("expected >", pos);
        ret = parse_type(pos + 1, &pos);
    }
    while (text_is(pos, "requires") || text_is(pos, "ensures") || text_is(pos, "uses")) {
        char *which = peek(pos)->text; pos = pos + 1;
        if (!kind_is(pos, "LParen")) perr("expected (", pos);
        pos = pos + 1;
        if (!strcmp(which, "uses")) {
            if (!kind_is(pos, "RParen")) {
                while (1) {
                    Token *et = peek(pos); pos = pos + 1;
                    Buf en = {0}; buf_puts(&en, et->text);
                    while (kind_is(pos, "Dot")) {
                        pos = pos + 1; Token *more = peek(pos); pos = pos + 1;
                        buf_putc(&en, '.'); buf_puts(&en, more->text);
                    }
                    buf_putc(&en, 0);
                    vec_push(&it->effects, en.d ? en.d : xstrdup(""));
                    if (kind_is(pos, "Comma")) pos = pos + 1; else break;
                }
            }
        } else {
            Expr *e = parse_expr(pos, 0, &pos);
            if (!strcmp(which, "requires")) vec_push(&it->requires_, e); else vec_push(&it->ensures_, e);
        }
        if (!kind_is(pos, "RParen")) perr("expected )", pos);
        pos = pos + 1;
    }
    Vec body = parse_block(pos, &pos);
    it->name = name_t->text; it->ret = ret; it->body = body;
    *out_pos = pos; return it;
}

/* sh_parse: top-level. Returns a Vec of Item*. On error, longjmps. */
static Vec sh_parse_items(void) {
    Vec items = {0};
    int pos = 0;
    while (!at_end(pos)) {
        Item *it;
        if (text_is(pos, "enum")) it = parse_enum(pos, &pos);
        else                      it = parse_fn(pos, &pos);
        vec_push(&items, it);
    }
    return items;
}

/* ---------------------------------------------------------------- emit ----- */
static Buf OUT;
static void emit(const char *s) { buf_puts(&OUT, s); }
static void emit_long(long v) { char tmp[32]; snprintf(tmp, sizeof(tmp), "%ld", v); buf_puts(&OUT, tmp); }
/* q(s): "..." with json_escape (backslash, dquote, \n, \t; else raw byte). */
static void emitq(const char *s) {
    buf_putc(&OUT, '"');
    for (const unsigned char *p = (const unsigned char *)s; *p; p++) {
        unsigned char c = *p;
        if (c == 92)      buf_puts(&OUT, "\\\\");
        else if (c == 34) buf_puts(&OUT, "\\\"");
        else if (c == 10) buf_puts(&OUT, "\\n");
        else if (c == 9)  buf_puts(&OUT, "\\t");
        else              buf_putc(&OUT, (char)c);
    }
    buf_putc(&OUT, '"');
}

static void jexpr(Expr *e);
static void jstmt(Stmt *s);

static void jstmts(Vec *ss) {
    emit("[");
    for (int i = 0; i < ss->n; i++) { if (i) emit(","); jstmt((Stmt *)ss->d[i]); }
    emit("]");
}

static void jpat(Pat *p) {
    if (!strcmp(p->pk, "wild")) { emit("{\"pk\":\"wild\"}"); return; }
    emit("{\"pk\":\"enum\",\"variant\":"); emitq(p->variant); emit(",\"shape\":"); emitq(p->shape);
    if (!strcmp(p->shape, "tuple")) {
        emit(",\"binds\":[");
        for (int i = 0; i < p->binds.n; i++) { if (i) emit(","); emitq((char *)p->binds.d[i]); }
        emit("]");
    }
    if (!strcmp(p->shape, "struct")) {
        emit(",\"fnames\":[");
        for (int i = 0; i < p->fnames.n; i++) { if (i) emit(","); emitq((char *)p->fnames.d[i]); }
        emit("],\"binds\":[");
        for (int i = 0; i < p->binds.n; i++) { if (i) emit(","); emitq((char *)p->binds.d[i]); }
        emit("]");
    }
    emit("}");
}

static void jexpr(Expr *e) {
    switch (e->kind) {
        case E_NONE: emit("null"); break;
        case E_INT:  emit("{\"kind\":\"Int\",\"value\":");  emitq(e->s); emit("}"); break;
        case E_STR:  emit("{\"kind\":\"Str\",\"value\":");  emitq(e->s); emit("}"); break;
        case E_BOOL: emit("{\"kind\":\"Bool\",\"value\":"); emit(e->bval ? "true" : "false"); emit("}"); break;
        case E_VAR:  emit("{\"kind\":\"Var\",\"name\":");    emitq(e->s); emit("}"); break;
        case E_CALL:
            emit("{\"kind\":\"Call\",\"callee\":"); emitq(e->callee); emit(",\"args\":[");
            for (int i = 0; i < e->args.n; i++) { if (i) emit(","); jexpr((Expr *)e->args.d[i]); }
            emit("]}");
            break;
        case E_BINARY:
            emit("{\"kind\":\"Binary\",\"op\":"); emitq(e->op);
            emit(",\"lhs\":"); jexpr(e->lhs); emit(",\"rhs\":"); jexpr(e->rhs); emit("}");
            break;
        case E_UNARY:
            emit("{\"kind\":\"Unary\",\"op\":"); emitq(e->op); emit(",\"expr\":"); jexpr(e->operand); emit("}");
            break;
        case E_INDEX:
            emit("{\"kind\":\"Index\",\"base\":"); jexpr(e->base); emit(",\"index\":"); jexpr(e->index); emit("}");
            break;
        case E_LIST:
            emit("{\"kind\":\"List\",\"elements\":[");
            for (int i = 0; i < e->elements.n; i++) { if (i) emit(","); jexpr((Expr *)e->elements.d[i]); }
            emit("]}");
            break;
        case E_RANGE:
            emit("{\"kind\":\"Range\",\"start\":"); jexpr(e->rstart); emit(",\"end\":"); jexpr(e->rend); emit("}");
            break;
        case E_MAP:
            emit("{\"kind\":\"Map\",\"keys\":[");
            for (int i = 0; i < e->keys.n; i++) { if (i) emit(","); emitq((char *)e->keys.d[i]); }
            emit("],\"vals\":[");
            for (int i = 0; i < e->vals.n; i++) { if (i) emit(","); jexpr((Expr *)e->vals.d[i]); }
            emit("]}");
            break;
        case E_ENUMINIT:
            emit("{\"kind\":\"EnumInit\",\"ty\":"); emitq(e->ei_ty);
            emit(",\"variant\":"); emitq(e->ei_variant);
            emit(",\"shape\":"); emitq(e->ei_shape);
            if (!strcmp(e->ei_shape, "tuple")) {
                emit(",\"tuple\":[");
                for (int i = 0; i < e->ei_tuple.n; i++) { if (i) emit(","); jexpr((Expr *)e->ei_tuple.d[i]); }
                emit("]");
            }
            if (!strcmp(e->ei_shape, "struct")) {
                emit(",\"fnames\":[");
                for (int i = 0; i < e->ei_fnames.n; i++) { if (i) emit(","); emitq((char *)e->ei_fnames.d[i]); }
                emit("],\"fexprs\":[");
                for (int i = 0; i < e->ei_fexprs.n; i++) { if (i) emit(","); jexpr((Expr *)e->ei_fexprs.d[i]); }
                emit("]");
            }
            emit("}");
            break;
        case E_IFEXPR:
            emit("{\"kind\":\"IfExpr\",\"cond\":"); jexpr(e->if_cond);
            emit(",\"then\":"); jstmts(&e->if_then);
            emit(",\"else\":"); jstmts(&e->if_else); emit("}");
            break;
        case E_MATCH:
            emit("{\"kind\":\"Match\",\"scrut\":"); jexpr(e->m_scrut); emit(",\"arms\":[");
            for (int i = 0; i < e->m_arms.n; i++) {
                if (i) emit(",");
                Arm *a = (Arm *)e->m_arms.d[i];
                emit("{\"pat\":"); jpat(a->pat); emit(",\"body\":"); jexpr(a->body); emit("}");
            }
            emit("]}");
            break;
        default: emit("{\"kind\":\"UnsupportedExpr\"}"); break;
    }
}

static void jstmt(Stmt *s) {
    switch (s->kind) {
        case S_LET:
            emit("{\"kind\":\"Let\",\"name\":"); emitq(s->let_name);
            emit(",\"ty\":"); if (strcmp(s->let_ty, "")) emitq(s->let_ty); else emit("null");
            emit(",\"init\":"); jexpr(s->let_init); emit("}");
            break;
        case S_ASSIGN:
            emit("{\"kind\":\"Assign\",\"target\":"); jexpr(s->as_target);
            emit(",\"value\":"); jexpr(s->as_value); emit("}");
            break;
        case S_RETURN:
            emit("{\"kind\":\"Return\",\"value\":"); jexpr(s->ret_value); emit("}");
            break;
        case S_EXPRSTMT:
            emit("{\"kind\":\"ExprStmt\",\"expr\":"); jexpr(s->es_expr); emit("}");
            break;
        case S_IF:
            emit("{\"kind\":\"If\",\"cond\":"); jexpr(s->if_cond);
            emit(",\"then\":"); jstmts(&s->if_then);
            emit(",\"else\":"); if (s->has_else) jstmts(&s->if_else); else emit("null"); emit("}");
            break;
        case S_WHILE:
            emit("{\"kind\":\"While\",\"cond\":"); jexpr(s->wh_cond);
            emit(",\"body\":"); jstmts(&s->wh_body); emit("}");
            break;
        case S_FOR:
            emit("{\"kind\":\"For\",\"var\":"); emitq(s->for_var);
            emit(",\"iter\":"); jexpr(s->for_iter);
            emit(",\"body\":"); jstmts(&s->for_body); emit("}");
            break;
        default: emit("{\"kind\":\"UnsupportedStmt\"}"); break;
    }
}

static void jfn(Item *f) {
    emit("{\"kind\":\"Fn\",\"name\":"); emitq(f->name);
    emit(",\"pub\":"); emit(f->is_pub ? "true" : "false");
    emit(",\"params\":[");
    for (int i = 0; i < f->params.n; i++) {
        if (i) emit(",");
        Param *p = (Param *)f->params.d[i];
        emit("{\"name\":"); emitq(p->name); emit(",\"ty\":");
        if (strcmp(p->ty, "")) emitq(p->ty); else emit("null");
        emit("}");
    }
    emit("],\"ret\":"); if (strcmp(f->ret, "")) emitq(f->ret); else emit("null");
    emit(",\"requires\":[");
    for (int i = 0; i < f->requires_.n; i++) { if (i) emit(","); jexpr((Expr *)f->requires_.d[i]); }
    emit("],\"ensures\":[");
    for (int i = 0; i < f->ensures_.n; i++) { if (i) emit(","); jexpr((Expr *)f->ensures_.d[i]); }
    emit("],\"effects\":[");
    for (int i = 0; i < f->effects.n; i++) { if (i) emit(","); emitq((char *)f->effects.d[i]); }
    emit("],\"body\":"); jstmts(&f->body); emit("}");
}

static void jenum(Item *it) {
    emit("{\"kind\":\"Enum\",\"name\":"); emitq(it->enum_name); emit(",\"variants\":[");
    for (int i = 0; i < it->variants.n; i++) {
        if (i) emit(",");
        Variant *v = (Variant *)it->variants.d[i];
        emit("{\"name\":"); emitq(v->name); emit(",\"shape\":"); emitq(v->shape); emit("}");
    }
    emit("]}");
}

static void jast(Vec *items) {
    emit("{\"kind\":\"Program\",\"items\":[");
    for (int i = 0; i < items->n; i++) {
        if (i) emit(",");
        Item *it = (Item *)items->d[i];
        if (it->is_enum) jenum(it); else jfn(it);
    }
    emit("]}");
}

/* --------------------------------------------------------------- tokens ---- */
static void jtok(Token *t) {
    emit("{\"kind\":"); emitq(t->kind);
    emit(",\"text\":"); emitq(t->text);
    emit(",\"start\":"); emit_long(t->start);
    emit(",\"end\":"); emit_long(t->end);
    emit("}");
}
static void jtokens(Vec *ts) {
    emit("[");
    for (int i = 0; i < ts->n; i++) { if (i) emit(","); jtok((Token *)ts->d[i]); }
    emit("]");
}

/* ---------------------------------------------------------------- main ----- */
static char *read_file(const char *path) {
    FILE *f = fopen(path, "rb");
    if (!f) { fprintf(stderr, "anubis_sh_parse: cannot open %s\n", path); exit(2); }
    if (fseek(f, 0, SEEK_END) != 0) { fclose(f); fprintf(stderr, "seek failed\n"); exit(2); }
    long sz = ftell(f);
    if (sz < 0) { fclose(f); fprintf(stderr, "tell failed\n"); exit(2); }
    rewind(f);
    char *buf = xmalloc((size_t)sz + 1);
    size_t got = fread(buf, 1, (size_t)sz, f);
    fclose(f);
    buf[got] = 0;
    return buf;
}

int main(int argc, char **argv) {
    if (argc < 3) {
        fprintf(stderr, "usage: anubis_sh_parse (parse|lex) <file.anb>\n");
        return 2;
    }
    const char *cmd = argv[1];
    const char *path = argv[2];
    char *src = read_file(path);

    Vec toks = sh_lex(src);
    g_toks = xmalloc((size_t)(toks.n ? toks.n : 1) * sizeof(Token));
    for (int i = 0; i < toks.n; i++) g_toks[i] = *(Token *)toks.d[i];
    g_ntok = toks.n;

    if (!strcmp(cmd, "lex")) {
        jtokens(&toks);
        buf_putc(&OUT, '\n');
        fwrite(OUT.d, 1, OUT.n, stdout);
        return 0;
    }
    if (!strcmp(cmd, "parse")) {
        if (setjmp(g_err_jmp)) {
            /* Mirror sh_parse's error string:
             *   err + " at pos " + pos + " tok " + kind + ":" + text + " @" + start */
            Token *t = (g_err_pos >= 0 && g_err_pos < g_ntok) ? &g_toks[g_err_pos] : &g_eof;
            printf("PARSE_ERROR: %s at pos %d tok %s:%s @%ld\n",
                   g_err_msg, g_err_pos, t->kind, t->text, t->start);
            return 1;
        }
        Vec items = sh_parse_items();
        jast(&items);
#ifdef ANUBIS_DDC_NEG_CONTROL
        /* Negative-control hook: the DDC gate compiles a second binary with this
         * defined and requires its output to DIVERGE from the real one, proving the
         * parser-faithfulness comparison is load-bearing. Never defined in real builds. */
        buf_putc(&OUT, 'X');
#endif
        buf_putc(&OUT, '\n');
        fwrite(OUT.d, 1, OUT.n, stdout);
        return 0;
    }
    fprintf(stderr, "unknown command: %s\n", cmd);
    return 2;
}
