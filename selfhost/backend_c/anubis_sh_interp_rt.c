/* Anubis-SH executable backend — AST interpreter in C (no external deps).
 *
 * SECOND, TOOLCHAIN-INDEPENDENT execution path for the self-host compiler,
 * used by the Diverse Double-Compiling (DDC) gate (Wheeler,
 * dwheeler.com/trusting-trust). It is a faithful port of the Rust reference
 * interpreter selfhost/runtime/anubis_sh_interp_rt.rs. The Rust interpreter is
 * compiled by rustc (LLVM); THIS interpreter is compiled by GCC (a genuinely
 * independent, non-LLVM toolchain). Requiring both to emit byte-identical
 * compiler output for the same input makes a single hidden toolchain
 * subversion implausible: it would have to exist, identically, in both.
 *
 * Semantics are matched to the Rust reference exactly (verified by the DDC
 * gate's byte comparison):
 *   * Three heap value kinds (Str/List/Map) are reference-counted with
 *     copy-on-write (Rust Rc + make_mut). The `x = x + rhs` accumulator gets
 *     an in-place append (H5b), keeping the ~280 KB payload build amortized
 *     O(1) rather than O(n^2). make_mut self-heals a shared buffer to rc==1
 *     after one clone, so aggressive rc bumps cost at most one extra copy.
 *   * Maps are stored key-sorted (mirrors the Rust BTreeMap iteration order).
 *   * String builtins index by byte (ASCII source, SUBSET.md), matching Rust.
 *   * `byte as char` / `char::from_u32` UTF-8 expansion is replicated so that
 *     any non-ASCII byte would round-trip identically to the Rust path.
 *
 * Memory: this is a one-shot compiler process. Values are allocated and never
 * freed (leak-on-exit); rc is used only to drive copy-on-write correctness,
 * not reclamation. This is deliberate and keeps the port small and obviously
 * correct.
 */
#define _GNU_SOURCE
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <pthread.h>

/* ------------------------------------------------------------------ */
/* Value representation                                               */
/* ------------------------------------------------------------------ */
typedef enum { T_NULL, T_BOOL, T_INT, T_STR, T_LIST, T_MAP, T_ENUM } Tag;

typedef struct V V;

typedef struct {
    uint32_t rc;
    size_t len;
    size_t cap;
    char *data; /* not NUL-terminated for internal use; grown by append */
} Str;

typedef struct {
    uint32_t rc;
    size_t len;
    size_t cap;
    V *items;
} List;

typedef struct {
    char *key;
    V *val;
} Pair;

typedef struct {
    uint32_t rc;
    size_t len;
    size_t cap;
    Pair *pairs; /* kept sorted by key (BTreeMap order) */
} Map;

typedef struct {
    char *ety;     /* enum type */
    char *variant; /* variant name */
    List *tuple;   /* positional/tuple fields (may be NULL/empty) */
    size_t nfields;
    Pair *fields;  /* named/struct fields (may be NULL/empty) */
} EnumV;

struct V {
    Tag tag;
    union {
        int64_t i;
        int b;
        Str *s;
        List *l;
        Map *m;
        EnumV *e;
    } u;
};

static void die(const char *msg) {
    fprintf(stderr, "%s\n", msg);
    exit(101);
}static void *xmalloc(size_t n) {
    void *p = malloc(n ? n : 1);
    if (!p) die("out of memory");
    return p;
}
static void *xrealloc(void *p, size_t n) {
    void *q = realloc(p, n ? n : 1);
    if (!q) die("out of memory");
    return q;
}

/* ---- constructors ---- */
static V v_null(void) { V v; v.tag = T_NULL; v.u.i = 0; return v; }
static V v_bool(int b) { V v; v.tag = T_BOOL; v.u.b = b ? 1 : 0; return v; }
static V v_int(int64_t n) { V v; v.tag = T_INT; v.u.i = n; return v; }

static Str *str_new_cap(size_t cap) {
    Str *s = xmalloc(sizeof(Str));
    s->rc = 1;
    s->len = 0;
    s->cap = cap < 8 ? 8 : cap;
    s->data = xmalloc(s->cap);
    return s;
}
static void str_reserve(Str *s, size_t extra) {
    if (s->len + extra > s->cap) {
        size_t nc = s->cap * 2;
        if (nc < s->len + extra) nc = s->len + extra;
        s->data = xrealloc(s->data, nc);
        s->cap = nc;
    }
}
static void str_push_bytes(Str *s, const char *b, size_t n) {
    str_reserve(s, n);
    memcpy(s->data + s->len, b, n);
    s->len += n;
}
static Str *str_from_bytes(const char *b, size_t n) {
    Str *s = str_new_cap(n);
    str_push_bytes(s, b, n);
    return s;
}
static Str *str_from_cstr(const char *c) { return str_from_bytes(c, strlen(c)); }

static V vs_bytes(const char *b, size_t n) { V v; v.tag = T_STR; v.u.s = str_from_bytes(b, n); return v; }
static V vs(const char *c) { V v; v.tag = T_STR; v.u.s = str_from_cstr(c); return v; }
static V vs_take(Str *s) { V v; v.tag = T_STR; v.u.s = s; return v; }

static List *list_new_cap(size_t cap) {
    List *l = xmalloc(sizeof(List));
    l->rc = 1;
    l->len = 0;
    l->cap = cap;
    l->items = cap ? xmalloc(sizeof(V) * cap) : NULL;
    return l;
}
static void list_push(List *l, V v) {
    if (l->len == l->cap) {
        size_t nc = l->cap ? l->cap * 2 : 4;
        l->items = xrealloc(l->items, sizeof(V) * nc);
        l->cap = nc;
    }
    l->items[l->len++] = v;
}
static V vl(List *l) { V v; v.tag = T_LIST; v.u.l = l; return v; }

static Map *map_new(void) {
    Map *m = xmalloc(sizeof(Map));
    m->rc = 1;
    m->len = 0;
    m->cap = 0;
    m->pairs = NULL;
    return m;
}
static V vm(Map *m) { V v; v.tag = T_MAP; v.u.m = m; return v; }

/* Binary search for key; returns index of match in *idx and 1, else insertion
 * point in *idx and 0. Mirrors BTreeMap<String,V> ordering (byte/lexicographic
 * on the key as a C string). */
static int map_find(Map *m, const char *key, size_t *idx) {
    size_t lo = 0, hi = m->len;
    while (lo < hi) {
        size_t mid = lo + (hi - lo) / 2;
        int c = strcmp(m->pairs[mid].key, key);
        if (c == 0) { *idx = mid; return 1; }
        if (c < 0) lo = mid + 1;
        else hi = mid;
    }
    *idx = lo;
    return 0;
}
static void map_insert(Map *m, const char *key, V val) {
    size_t idx;
    if (map_find(m, key, &idx)) {
        m->pairs[idx].val = xmalloc(sizeof(V));
        *m->pairs[idx].val = val;
        return;
    }
    if (m->len == m->cap) {
        size_t nc = m->cap ? m->cap * 2 : 4;
        m->pairs = xrealloc(m->pairs, sizeof(Pair) * nc);
        m->cap = nc;
    }
    memmove(&m->pairs[idx + 1], &m->pairs[idx], sizeof(Pair) * (m->len - idx));
    m->pairs[idx].key = strdup(key);
    m->pairs[idx].val = xmalloc(sizeof(V));
    *m->pairs[idx].val = val;
    m->len++;
}
static V *map_lookup(Map *m, const char *key) {
    size_t idx;
    if (map_find(m, key, &idx)) return m->pairs[idx].val;
    return NULL;
}

/* ---- shallow clone with rc bump (mirrors Rust V::clone for Rc kinds) ---- */
static V v_clone(V v) {
    switch (v.tag) {
        case T_STR: v.u.s->rc++; break;
        case T_LIST: v.u.l->rc++; break;
        case T_MAP: v.u.m->rc++; break;
        default: break; /* Null/Bool/Int/Enum: bitwise copy (Enum is immutable) */
    }
    return v;
}

/* ---- copy-on-write make_mut ---- */
static Str *str_make_mut(V *slot) {
    Str *s = slot->u.s;
    if (s->rc == 1) return s;
    Str *ns = str_from_bytes(s->data, s->len);
    slot->u.s = ns;
    return ns;
}
static List *list_make_mut(V *slot) {
    List *l = slot->u.l;
    if (l->rc == 1) return l;
    List *nl = list_new_cap(l->len);
    for (size_t i = 0; i < l->len; i++) nl->items[i] = l->items[i];
    nl->len = l->len;
    slot->u.l = nl;
    return nl;
}
static Map *map_make_mut(V *slot) {
    Map *m = slot->u.m;
    if (m->rc == 1) return m;
    Map *nm = map_new();
    nm->cap = m->cap;
    nm->len = m->len;
    nm->pairs = m->len ? xmalloc(sizeof(Pair) * m->cap) : NULL;
    for (size_t i = 0; i < m->len; i++) {
        nm->pairs[i].key = m->pairs[i].key; /* keys are immutable, share */
        nm->pairs[i].val = m->pairs[i].val;
    }
    slot->u.m = nm;
    return nm;
}

/* ------------------------------------------------------------------ */
/* UTF-8 helpers to replicate Rust `byte as char` / char::from_u32     */
/* ------------------------------------------------------------------ */
static void push_codepoint(Str *s, uint32_t cp) {
    char buf[4];
    size_t n;
    if (cp < 0x80) { buf[0] = (char)cp; n = 1; }
    else if (cp < 0x800) {
        buf[0] = (char)(0xC0 | (cp >> 6));
        buf[1] = (char)(0x80 | (cp & 0x3F));
        n = 2;
    } else if (cp < 0x10000) {
        buf[0] = (char)(0xE0 | (cp >> 12));
        buf[1] = (char)(0x80 | ((cp >> 6) & 0x3F));
        buf[2] = (char)(0x80 | (cp & 0x3F));
        n = 3;
    } else {
        buf[0] = (char)(0xF0 | (cp >> 18));
        buf[1] = (char)(0x80 | ((cp >> 12) & 0x3F));
        buf[2] = (char)(0x80 | ((cp >> 6) & 0x3F));
        buf[3] = (char)(0x80 | (cp & 0x3F));
        n = 4;
    }
    str_push_bytes(s, buf, n);
}

/* ------------------------------------------------------------------ */
/* display / truthy / as_i64                                          */
/* ------------------------------------------------------------------ */
static void display_into(Str *out, const V *v);

static void display_into(Str *out, const V *v) {
    char nb[32];
    switch (v->tag) {
        case T_NULL: break; /* empty */
        case T_BOOL: str_push_bytes(out, v->u.b ? "true" : "false", v->u.b ? 4 : 5); break;
        case T_INT: {
            int n = snprintf(nb, sizeof nb, "%lld", (long long)v->u.i);
            str_push_bytes(out, nb, (size_t)n);
            break;
        }
        case T_STR: str_push_bytes(out, v->u.s->data, v->u.s->len); break;
        case T_LIST: {
            str_push_bytes(out, "[", 1);
            for (size_t i = 0; i < v->u.l->len; i++) {
                if (i) str_push_bytes(out, ", ", 2);
                display_into(out, &v->u.l->items[i]);
            }
            str_push_bytes(out, "]", 1);
            break;
        }
        case T_MAP: str_push_bytes(out, "{...}", 5); break;
        case T_ENUM: {
            EnumV *e = v->u.e;
            if (e->tuple && e->tuple->len > 0) {
                str_push_bytes(out, e->variant, strlen(e->variant));
                str_push_bytes(out, "(", 1);
                for (size_t i = 0; i < e->tuple->len; i++) {
                    if (i) str_push_bytes(out, ", ", 2);
                    display_into(out, &e->tuple->items[i]);
                }
                str_push_bytes(out, ")", 1);
            } else if (e->nfields > 0) {
                str_push_bytes(out, e->variant, strlen(e->variant));
                str_push_bytes(out, " { ", 3);
                for (size_t i = 0; i < e->nfields; i++) {
                    if (i) str_push_bytes(out, ", ", 2);
                    str_push_bytes(out, e->fields[i].key, strlen(e->fields[i].key));
                    str_push_bytes(out, ": ", 2);
                    display_into(out, e->fields[i].val);
                }
                str_push_bytes(out, " }", 2);
            } else {
                str_push_bytes(out, e->variant, strlen(e->variant));
            }
            break;
        }
    }
}
static Str *display_str(const V *v) {
    Str *s = str_new_cap(16);
    display_into(s, v);
    return s;
}
static int truthy(const V *v) {
    switch (v->tag) {
        case T_NULL: return 0;
        case T_BOOL: return v->u.b;
        case T_INT: return v->u.i != 0;
        case T_STR: return v->u.s->len != 0;
        case T_LIST: return v->u.l->len != 0;
        case T_MAP: return v->u.m->len != 0;
        case T_ENUM: return 1;
    }
    return 0;
}
static int64_t as_i64(const V *v) {
    switch (v->tag) {
        case T_INT: return v->u.i;
        case T_BOOL: return v->u.b ? 1 : 0;
        case T_STR: {
            /* parse leading integer like Rust str::parse::<i64>(): whole string
             * must be a valid i64 else 0. Use strtoll on a NUL-terminated copy. */
            Str *s = v->u.s;
            char *tmp = xmalloc(s->len + 1);
            memcpy(tmp, s->data, s->len);
            tmp[s->len] = 0;
            char *end;
            long long r = strtoll(tmp, &end, 10);
            if (end == tmp || *end != 0) return 0;
            return (int64_t)r;
        }
        default: return 0;
    }
}

/* as_str_val: Str -> its bytes; else display. Returns a heap Str. */
static Str *as_str_val(const V *v) {
    if (v->tag == T_STR) return v->u.s;
    return display_str(v);
}

/* ------------------------------------------------------------------ */
/* JSON parser (mirrors Jp)                                            */
/* ------------------------------------------------------------------ */
typedef struct {
    const unsigned char *s;
    size_t n;
    size_t i;
} Jp;

static V jp_parse(Jp *p);

static void jp_skip_ws(Jp *p) {
    while (p->i < p->n) {
        unsigned char c = p->s[p->i];
        if (c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == '\f' || c == '\v') p->i++;
        else break;
    }
}
static unsigned char jp_peek(Jp *p) {
    jp_skip_ws(p);
    return p->i < p->n ? p->s[p->i] : 0;
}
static unsigned char jp_bump(Jp *p) {
    jp_skip_ws(p);
    return p->s[p->i++];
}
static Str *jp_string(Jp *p) {
    if (jp_bump(p) != '"') die("json: expected string");
    Str *out = str_new_cap(16);
    while (p->i < p->n) {
        unsigned char c = p->s[p->i++];
        if (c == '"') break;
        if (c == '\\') {
            unsigned char e = p->s[p->i++];
            switch (e) {
                case 'n': str_push_bytes(out, "\n", 1); break;
                case 't': str_push_bytes(out, "\t", 1); break;
                case 'r': str_push_bytes(out, "\r", 1); break;
                case '"': str_push_bytes(out, "\"", 1); break;
                case '\\': str_push_bytes(out, "\\", 1); break;
                case '/': str_push_bytes(out, "/", 1); break;
                case 'u': {
                    char hex[5];
                    for (int k = 0; k < 4; k++) hex[k] = (char)p->s[p->i + k];
                    hex[4] = 0;
                    p->i += 4;
                    unsigned long v = strtoul(hex, NULL, 16);
                    /* char::from_u32: skip surrogate range (returns None -> nothing pushed) */
                    if (!(v >= 0xD800 && v <= 0xDFFF) && v <= 0x10FFFF)
                        push_codepoint(out, (uint32_t)v);
                    break;
                }
                default: push_codepoint(out, (uint32_t)e); break; /* Rust: e as char */
            }
        } else {
            push_codepoint(out, (uint32_t)c); /* Rust: c as char */
        }
    }
    return out;
}
static V jp_number(Jp *p) {
    jp_skip_ws(p);
    size_t start = p->i;
    if (p->i < p->n && p->s[p->i] == '-') p->i++;
    while (p->i < p->n && p->s[p->i] >= '0' && p->s[p->i] <= '9') p->i++;
    char tmp[32];
    size_t len = p->i - start;
    if (len >= sizeof tmp) len = sizeof tmp - 1;
    memcpy(tmp, p->s + start, len);
    tmp[len] = 0;
    return v_int((int64_t)strtoll(tmp, NULL, 10));
}
static V jp_array(Jp *p) {
    if (jp_bump(p) != '[') die("json: expected [");
    List *l = list_new_cap(4);
    if (jp_peek(p) == ']') { jp_bump(p); return vl(l); }
    for (;;) {
        list_push(l, jp_parse(p));
        if (jp_peek(p) == ',') { jp_bump(p); continue; }
        break;
    }
    if (jp_bump(p) != ']') die("json: expected ]");
    return vl(l);
}
static V jp_object(Jp *p) {
    if (jp_bump(p) != '{') die("json: expected {");
    Map *m = map_new();
    if (jp_peek(p) == '}') { jp_bump(p); return vm(m); }
    for (;;) {
        Str *k = jp_string(p);
        if (jp_bump(p) != ':') die("json: expected :");
        V v = jp_parse(p);
        char *key = xmalloc(k->len + 1);
        memcpy(key, k->data, k->len);
        key[k->len] = 0;
        map_insert(m, key, v);
        if (jp_peek(p) == ',') { jp_bump(p); continue; }
        break;
    }
    if (jp_bump(p) != '}') die("json: expected }");
    return vm(m);
}
static V jp_parse(Jp *p) {
    unsigned char c = jp_peek(p);
    switch (c) {
        case 'n': p->i += 4; return v_null();
        case 't': p->i += 4; return v_bool(1);
        case 'f': p->i += 5; return v_bool(0);
        case '"': return vs_take(jp_string(p));
        case '[': return jp_array(p);
        case '{': return jp_object(p);
        default:
            if (c == '-' || (c >= '0' && c <= '9')) return jp_number(p);
            die("json: unexpected char");
    }
    return v_null();
}
static V parse_json(const char *s, size_t n) {
    Jp p = { (const unsigned char *)s, n, 0 };
    return jp_parse(&p);
}

/* map_get equivalent: field access on a V::Map by key; returns &V::Null if absent/non-map. */
static V NULL_V;
static const V *map_get(const V *m, const char *k) {
    if (m->tag != T_MAP) return &NULL_V;
    V *r = map_lookup(m->u.m, k);
    return r ? r : &NULL_V;
}
/* as_str_val on a field, returned as a NUL-terminated C string (heap). */
static char *field_cstr(const V *m, const char *k) {
    const V *v = map_get(m, k);
    Str *s = as_str_val(v);
    char *c = xmalloc(s->len + 1);
    memcpy(c, s->data, s->len);
    c[s->len] = 0;
    return c;
}

/* ------------------------------------------------------------------ */
/* Interpreter                                                        */
/* ------------------------------------------------------------------ */
typedef enum { F_VAL, F_RETURN, F_BREAK, F_CONTINUE } FlowKind;
typedef struct { FlowKind kind; V v; } Flow;

typedef struct {
    V program;
    Map *fns; /* name -> V (Fn item) */
    int exit_code;
    int should_exit;
} Rt;

/* locals is a Map (string -> V), same as interp_rt's BTreeMap. */
static Flow exec_stmts(Rt *rt, const List *stmts, Map *locals, List *argv);
static Flow exec_stmt(Rt *rt, const V *st, Map *locals, List *argv);
static V eval(Rt *rt, const V *e, Map *locals, List *argv);
static V call_fn(Rt *rt, const char *name, List *args, List *argv);

static int streq(const char *a, const char *b) { return strcmp(a, b) == 0; }

/* Borrow a Str's bytes if v is Str (no clone). */
static Str *str_ref(const V *v) { return v->tag == T_STR ? v->u.s : NULL; }

static V eval_bin(const char *op, const V *l, const V *r) {
    if (streq(op, "+")) {
        if (l->tag == T_STR || r->tag == T_STR) {
            Str *out = str_new_cap(16);
            display_into(out, l);
            display_into(out, r);
            return vs_take(out);
        }
        return v_int(as_i64(l) + as_i64(r));
    }
    if (streq(op, "-")) return v_int(as_i64(l) - as_i64(r));
    if (streq(op, "*")) return v_int(as_i64(l) * as_i64(r));
    if (streq(op, "/")) { int64_t d = as_i64(r); return v_int(d == 0 ? 0 : as_i64(l) / d); }
    if (streq(op, "%")) { int64_t d = as_i64(r); return v_int(d == 0 ? 0 : as_i64(l) % d); }
    if (streq(op, "==") || streq(op, "!=")) {
        int eq;
        if (l->tag == T_INT && r->tag == T_INT) eq = (l->u.i == r->u.i);
        else if (l->tag == T_BOOL && r->tag == T_BOOL) eq = (l->u.b == r->u.b);
        else if (l->tag == T_STR && r->tag == T_STR)
            eq = (l->u.s->len == r->u.s->len && memcmp(l->u.s->data, r->u.s->data, l->u.s->len) == 0);
        else {
            Str *ls = display_str(l), *rs = display_str(r);
            eq = (ls->len == rs->len && memcmp(ls->data, rs->data, ls->len) == 0);
        }
        return v_bool(streq(op, "==") ? eq : !eq);
    }
    if (streq(op, "<")) return v_bool(as_i64(l) < as_i64(r));
    if (streq(op, "<=")) return v_bool(as_i64(l) <= as_i64(r));
    if (streq(op, ">")) return v_bool(as_i64(l) > as_i64(r));
    if (streq(op, ">=")) return v_bool(as_i64(l) >= as_i64(r));
    if (streq(op, "&&")) return v_bool(truthy(l) && truthy(r));
    if (streq(op, "||")) return v_bool(truthy(l) || truthy(r));
    return v_null();
}

/* Try to match a pattern against a value; returns 1 and fills binds on success. */
static int match_pat(const V *pat, const V *val, Map *out_binds) {
    char *pk = field_cstr(pat, "pk");
    if (streq(pk, "wild")) return 1;
    if (streq(pk, "enum")) {
        char *want = field_cstr(pat, "variant");
        if (val->tag != T_ENUM) return 0;
        EnumV *e = val->u.e;
        if (!streq(e->variant, want)) return 0;
        char *shape = field_cstr(pat, "shape");
        if (streq(shape, "tuple")) {
            const V *names = map_get(pat, "binds");
            if (names->tag == T_LIST) {
                for (size_t i = 0; i < names->u.l->len; i++) {
                    Str *nm = as_str_val(&names->u.l->items[i]);
                    if (!(nm->len == 1 && nm->data[0] == '_')) {
                        char *key = xmalloc(nm->len + 1);
                        memcpy(key, nm->data, nm->len); key[nm->len] = 0;
                        V bv = (e->tuple && i < e->tuple->len) ? v_clone(e->tuple->items[i]) : v_null();
                        map_insert(out_binds, key, bv);
                    }
                }
            }
        } else if (streq(shape, "struct")) {
            const V *fnames = map_get(pat, "fnames");
            const V *bnames = map_get(pat, "binds");
            size_t fn = fnames->tag == T_LIST ? fnames->u.l->len : 0;
            size_t bn = bnames->tag == T_LIST ? bnames->u.l->len : 0;
            for (size_t i = 0; i < fn; i++) {
                Str *fns = as_str_val(&fnames->u.l->items[i]);
                char *fnamec = xmalloc(fns->len + 1); memcpy(fnamec, fns->data, fns->len); fnamec[fns->len] = 0;
                char *bname;
                if (i < bn) {
                    Str *bns = as_str_val(&bnames->u.l->items[i]);
                    bname = xmalloc(bns->len + 1); memcpy(bname, bns->data, bns->len); bname[bns->len] = 0;
                } else {
                    bname = strdup(fnamec);
                }
                if (!streq(bname, "_")) {
                    V fv = v_null();
                    for (size_t j = 0; j < e->nfields; j++)
                        if (streq(e->fields[j].key, fnamec)) { fv = v_clone(*e->fields[j].val); break; }
                    map_insert(out_binds, bname, fv);
                }
            }
        }
        return 1;
    }
    return 0;
}

static V call_fn(Rt *rt, const char *name, List *args, List *argv) {
    if (rt->should_exit) return v_null();
    /* builtins */
    if (streq(name, "print") || streq(name, "println")) {
        if (args->len >= 1) {
            Str *s = display_str(&args->items[0]);
            fwrite(s->data, 1, s->len, stdout);
            fputc('\n', stdout);
        }
        return v_null();
    }
    if (streq(name, "len")) {
        int64_t n = 0;
        if (args->len >= 1) {
            V *a = &args->items[0];
            if (a->tag == T_STR) n = (int64_t)a->u.s->len;
            else if (a->tag == T_LIST) n = (int64_t)a->u.l->len;
            else if (a->tag == T_MAP) n = (int64_t)a->u.m->len;
        }
        return v_int(n);
    }
    if (streq(name, "char_at")) {
        int64_t i = args->len > 1 ? as_i64(&args->items[1]) : 0;
        if (i >= 0 && args->len >= 1) {
            Str *s = str_ref(&args->items[0]);
            if (s && (size_t)i < s->len) {
                Str *out = str_new_cap(4);
                push_codepoint(out, (uint32_t)(unsigned char)s->data[i]);
                return vs_take(out);
            }
        }
        return vs("");
    }
    if (streq(name, "ord")) {
        Str *s = args->len >= 1 ? str_ref(&args->items[0]) : NULL;
        if (s && s->len > 0) return v_int((int64_t)(unsigned char)s->data[0]);
        return v_int(0);
    }
    if (streq(name, "chr")) {
        int64_t n = args->len >= 1 ? as_i64(&args->items[0]) : 0;
        uint32_t cp = (uint32_t)n;
        Str *out = str_new_cap(4);
        if (!(cp >= 0xD800 && cp <= 0xDFFF) && cp <= 0x10FFFF) push_codepoint(out, cp);
        else push_codepoint(out, 0); /* char::from_u32 None -> unwrap_or('\0') */
        return vs_take(out);
    }
    if (streq(name, "substr")) {
        int64_t st = args->len > 1 ? as_i64(&args->items[1]) : 0;
        int64_t ln = args->len > 2 ? as_i64(&args->items[2]) : 0;
        if (st < 0) st = 0;
        if (ln < 0) ln = 0;
        Str *s = args->len >= 1 ? str_ref(&args->items[0]) : NULL;
        if (s) {
            size_t start = (size_t)st; if (start > s->len) start = s->len;
            size_t end = start + (size_t)ln; if (end > s->len) end = s->len;
            return vs_bytes(s->data + start, end - start);
        }
        return vs("");
    }
    if (streq(name, "index_of")) {
        Str *sub = as_str_val(args->len > 1 ? &args->items[1] : &NULL_V);
        Str *s = args->len >= 1 ? str_ref(&args->items[0]) : NULL;
        if (s) {
            if (sub->len == 0) return v_int(0);
            if (sub->len <= s->len) {
                for (size_t i = 0; i + sub->len <= s->len; i++)
                    if (memcmp(s->data + i, sub->data, sub->len) == 0) return v_int((int64_t)i);
            }
            return v_int(-1);
        }
        return v_int(-1);
    }
    if (streq(name, "split")) {
        Str *s = as_str_val(args->len >= 1 ? &args->items[0] : &NULL_V);
        Str *sep = as_str_val(args->len > 1 ? &args->items[1] : &NULL_V);
        List *parts = list_new_cap(4);
        if (sep->len == 0) {
            /* chars(): iterate UTF-8 scalar values */
            size_t i = 0;
            while (i < s->len) {
                unsigned char c = (unsigned char)s->data[i];
                size_t clen = 1;
                if (c >= 0xF0) clen = 4; else if (c >= 0xE0) clen = 3; else if (c >= 0xC0) clen = 2;
                if (i + clen > s->len) clen = 1;
                list_push(parts, vs_bytes(s->data + i, clen));
                i += clen;
            }
        } else {
            size_t start = 0;
            for (size_t i = 0; i + sep->len <= s->len;) {
                if (memcmp(s->data + i, sep->data, sep->len) == 0) {
                    list_push(parts, vs_bytes(s->data + start, i - start));
                    i += sep->len;
                    start = i;
                } else {
                    i++;
                }
            }
            list_push(parts, vs_bytes(s->data + start, s->len - start));
        }
        return vl(parts);
    }
    if (streq(name, "push")) {
        List *nl = list_new_cap(0);
        if (args->len >= 1 && args->items[0].tag == T_LIST) {
            List *src = args->items[0].u.l;
            nl = list_new_cap(src->len + 1);
            for (size_t i = 0; i < src->len; i++) nl->items[i] = src->items[i];
            nl->len = src->len;
        }
        if (args->len > 1) list_push(nl, args->items[1]);
        return vl(nl);
    }
    if (streq(name, "get")) {
        const V *m = args->len >= 1 ? &args->items[0] : &NULL_V;
        Str *k = as_str_val(args->len > 1 ? &args->items[1] : &NULL_V);
        V def = args->len > 2 ? v_clone(args->items[2]) : v_null();
        if (m->tag == T_MAP) {
            char *kc = xmalloc(k->len + 1); memcpy(kc, k->data, k->len); kc[k->len] = 0;
            V *r = map_lookup(m->u.m, kc);
            return r ? v_clone(*r) : def;
        }
        return def;
    }
    if (streq(name, "has_key")) {
        const V *m = args->len >= 1 ? &args->items[0] : &NULL_V;
        Str *k = as_str_val(args->len > 1 ? &args->items[1] : &NULL_V);
        if (m->tag == T_MAP) {
            char *kc = xmalloc(k->len + 1); memcpy(kc, k->data, k->len); kc[k->len] = 0;
            return v_bool(map_lookup(m->u.m, kc) != NULL);
        }
        return v_bool(0);
    }
    if (streq(name, "keys")) {
        List *l = list_new_cap(4);
        if (args->len >= 1 && args->items[0].tag == T_MAP) {
            Map *m = args->items[0].u.m;
            for (size_t i = 0; i < m->len; i++) list_push(l, vs(m->pairs[i].key));
        }
        return vl(l);
    }
    if (streq(name, "values")) {
        List *l = list_new_cap(4);
        if (args->len >= 1 && args->items[0].tag == T_MAP) {
            Map *m = args->items[0].u.m;
            for (size_t i = 0; i < m->len; i++) list_push(l, v_clone(*m->pairs[i].val));
        }
        return vl(l);
    }
    if (streq(name, "read_file")) {
        Str *p = as_str_val(args->len >= 1 ? &args->items[0] : &NULL_V);
        char *pc = xmalloc(p->len + 1); memcpy(pc, p->data, p->len); pc[p->len] = 0;
        FILE *f = fopen(pc, "rb");
        if (!f) { fprintf(stderr, "read_file %s: cannot open\n", pc); exit(101); }
        Str *out = str_new_cap(4096);
        char buf[65536];
        size_t r;
        while ((r = fread(buf, 1, sizeof buf, f)) > 0) str_push_bytes(out, buf, r);
        fclose(f);
        return vs_take(out);
    }
    if (streq(name, "write_file")) {
        Str *p = as_str_val(args->len >= 1 ? &args->items[0] : &NULL_V);
        Str *d = as_str_val(args->len > 1 ? &args->items[1] : &NULL_V);
        char *pc = xmalloc(p->len + 1); memcpy(pc, p->data, p->len); pc[p->len] = 0;
        FILE *f = fopen(pc, "wb");
        if (!f) die("write_file");
        fwrite(d->data, 1, d->len, f);
        fclose(f);
        return v_null();
    }
    if (streq(name, "append_file")) {
        Str *p = as_str_val(args->len >= 1 ? &args->items[0] : &NULL_V);
        Str *d = as_str_val(args->len > 1 ? &args->items[1] : &NULL_V);
        char *pc = xmalloc(p->len + 1); memcpy(pc, p->data, p->len); pc[p->len] = 0;
        FILE *f = fopen(pc, "ab");
        if (f) { fwrite(d->data, 1, d->len, f); fclose(f); }
        return v_null();
    }
    if (streq(name, "args")) {
        List *l = list_new_cap(argv->len);
        for (size_t i = 0; i < argv->len; i++) list_push(l, v_clone(argv->items[i]));
        return vl(l);
    }
    if (streq(name, "env")) {
        Str *k = as_str_val(args->len >= 1 ? &args->items[0] : &NULL_V);
        char *kc = xmalloc(k->len + 1); memcpy(kc, k->data, k->len); kc[k->len] = 0;
        char *val = getenv(kc);
        return vs(val ? val : "");
    }
    if (streq(name, "declassify")) return args->len >= 1 ? v_clone(args->items[0]) : v_null();
    if (streq(name, "exit")) {
        rt->exit_code = args->len >= 1 ? (int)as_i64(&args->items[0]) : 0;
        rt->should_exit = 1;
        return v_null();
    }
    if (streq(name, "break")) return vs("__break__");
    if (streq(name, "continue")) return vs("__continue__");

    /* user function */
    V *f = map_lookup(rt->fns, name);
    if (!f) { fprintf(stderr, "unknown function %s\n", name); exit(101); }
    Map *locals = map_new();
    const V *params = map_get(f, "params");
    if (params->tag == T_LIST) {
        for (size_t i = 0; i < params->u.l->len; i++) {
            char *pname = field_cstr(&params->u.l->items[i], "name");
            V av = i < args->len ? v_clone(args->items[i]) : v_null();
            map_insert(locals, pname, av);
        }
    }
    const V *body = map_get(f, "body");
    Flow flow;
    if (body->tag == T_LIST) flow = exec_stmts(rt, body->u.l, locals, argv);
    else { flow.kind = F_VAL; flow.v = v_null(); }
    if (flow.kind == F_RETURN || flow.kind == F_VAL) return flow.v;
    return v_null();
}

static Flow flow_val(V v) { Flow f; f.kind = F_VAL; f.v = v; return f; }
static Flow flow_ret(V v) { Flow f; f.kind = F_RETURN; f.v = v; return f; }

static Flow exec_stmts(Rt *rt, const List *stmts, Map *locals, List *argv) {
    V last = v_null();
    for (size_t i = 0; i < stmts->len; i++) {
        if (rt->should_exit) return flow_ret(v_null());
        Flow f = exec_stmt(rt, &stmts->items[i], locals, argv);
        if (f.kind == F_RETURN) return f;
        if (f.kind == F_BREAK || f.kind == F_CONTINUE) return f;
        last = f.v;
    }
    return flow_val(last);
}

static V eval_block(Rt *rt, const List *stmts, Map *locals, List *argv) {
    Flow f = exec_stmts(rt, stmts, locals, argv);
    if (f.kind == F_VAL || f.kind == F_RETURN) return f.v;
    return v_null();
}

/* assign to an lvalue target (Var or Index). */
static void do_assign(Rt *rt, const V *target, V val, Map *locals, List *argv) {
    char *k = field_cstr(target, "kind");
    if (streq(k, "Var")) {
        char *n = field_cstr(target, "name");
        map_insert(locals, n, val);
        return;
    }
    if (streq(k, "Index")) {
        const V *base_e = map_get(target, "base");
        const V *idx_e = map_get(target, "index");
        if (streq(field_cstr(base_e, "kind"), "Var")) {
            char *n = field_cstr(base_e, "name");
            V idx = eval(rt, idx_e, locals, argv);
            V *slot = map_lookup(locals, n);
            if (slot) {
                if (slot->tag == T_MAP) {
                    Map *m = map_make_mut(slot);
                    Str *ks = as_str_val(&idx);
                    char *kc = xmalloc(ks->len + 1); memcpy(kc, ks->data, ks->len); kc[ks->len] = 0;
                    map_insert(m, kc, val);
                } else if (slot->tag == T_LIST) {
                    List *xs = list_make_mut(slot);
                    int64_t i = as_i64(&idx);
                    if (i >= 0 && (size_t)i < xs->len) xs->items[i] = val;
                }
            }
        }
    }
}

static Flow exec_stmt(Rt *rt, const V *st, Map *locals, List *argv) {
    char *k = field_cstr(st, "kind");
    if (streq(k, "Let")) {
        char *name = field_cstr(st, "name");
        V init = eval(rt, map_get(st, "init"), locals, argv);
        map_insert(locals, name, init);
        return flow_val(v_null());
    }
    if (streq(k, "Assign")) {
        const V *t = map_get(st, "target");
        /* H5b: in-place string append for `x = x + rhs` when x is a string. */
        if (streq(field_cstr(t, "kind"), "Var")) {
            char *tname = field_cstr(t, "name");
            const V *ve = map_get(st, "value");
            if (streq(field_cstr(ve, "kind"), "Binary") && streq(field_cstr(ve, "op"), "+")) {
                const V *lhs = map_get(ve, "lhs");
                if (streq(field_cstr(lhs, "kind"), "Var") && streq(field_cstr(lhs, "name"), tname)) {
                    V *slot = map_lookup(locals, tname);
                    if (slot && slot->tag == T_STR) {
                        V rhs = eval(rt, map_get(ve, "rhs"), locals, argv);
                        Str *piece = display_str(&rhs);
                        Str *s = str_make_mut(slot);
                        str_push_bytes(s, piece->data, piece->len);
                        return flow_val(v_null());
                    }
                }
            }
        }
        V val = eval(rt, map_get(st, "value"), locals, argv);
        do_assign(rt, t, val, locals, argv);
        return flow_val(v_null());
    }
    if (streq(k, "Return")) {
        const V *v = map_get(st, "value");
        if (streq(field_cstr(v, "kind"), "None")) return flow_ret(v_null());
        return flow_ret(eval(rt, v, locals, argv));
    }
    if (streq(k, "ExprStmt")) {
        V v = eval(rt, map_get(st, "expr"), locals, argv);
        if (v.tag == T_STR) {
            if (v.u.s->len == 9 && memcmp(v.u.s->data, "__break__", 9) == 0) return (Flow){ F_BREAK, v_null() };
            if (v.u.s->len == 12 && memcmp(v.u.s->data, "__continue__", 12) == 0) return (Flow){ F_CONTINUE, v_null() };
        }
        return flow_val(v);
    }
    if (streq(k, "If")) {
        V cond = eval(rt, map_get(st, "cond"), locals, argv);
        if (truthy(&cond)) {
            const V *body = map_get(st, "then");
            if (body->tag == T_LIST) return exec_stmts(rt, body->u.l, locals, argv);
        } else {
            const V *else_node = map_get(st, "else");
            if (else_node->tag == T_LIST) return exec_stmts(rt, else_node->u.l, locals, argv);
            const V *he = map_get(st, "has_else");
            if (truthy(he)) {
                const V *body = map_get(st, "else");
                if (body->tag == T_LIST) return exec_stmts(rt, body->u.l, locals, argv);
            }
        }
        return flow_val(v_null());
    }
    if (streq(k, "While")) {
        uint64_t guard = 0;
        for (;;) {
            guard++;
            if (guard > 50000000ULL) die("while loop exceeded 5e7 iterations (possible non-termination)");
            if (rt->should_exit) break;
            V cond = eval(rt, map_get(st, "cond"), locals, argv);
            if (!truthy(&cond)) break;
            const V *body = map_get(st, "body");
            if (body->tag == T_LIST) {
                Flow f = exec_stmts(rt, body->u.l, locals, argv);
                if (f.kind == F_RETURN) return f;
                if (f.kind == F_BREAK) break;
                if (f.kind == F_CONTINUE) continue;
            }
        }
        return flow_val(v_null());
    }
    if (streq(k, "For")) {
        const V *iter = map_get(st, "iter");
        char *var = field_cstr(st, "var");
        if (streq(field_cstr(iter, "kind"), "Range")) {
            V sv = eval(rt, map_get(iter, "start"), locals, argv);
            V ev = eval(rt, map_get(iter, "end"), locals, argv);
            int64_t i = as_i64(&sv), end = as_i64(&ev);
            const V *body = map_get(st, "body");
            const List *bl = body->tag == T_LIST ? body->u.l : NULL;
            while (i < end) {
                map_insert(locals, strdup(var), v_int(i));
                if (bl) {
                    Flow f = exec_stmts(rt, bl, locals, argv);
                    if (f.kind == F_RETURN) return f;
                    if (f.kind == F_BREAK) break;
                    if (f.kind == F_CONTINUE) { i++; continue; }
                }
                i++;
            }
            return flow_val(v_null());
        }
        V coll = eval(rt, iter, locals, argv);
        List *seq = list_new_cap(4);
        if (coll.tag == T_LIST) {
            for (size_t j = 0; j < coll.u.l->len; j++) list_push(seq, coll.u.l->items[j]);
        } else if (coll.tag == T_MAP) {
            for (size_t j = 0; j < coll.u.m->len; j++) list_push(seq, vs(coll.u.m->pairs[j].key));
        } else if (coll.tag == T_STR) {
            Str *s = coll.u.s;
            size_t j = 0;
            while (j < s->len) {
                unsigned char c = (unsigned char)s->data[j];
                size_t clen = 1;
                if (c >= 0xF0) clen = 4; else if (c >= 0xE0) clen = 3; else if (c >= 0xC0) clen = 2;
                if (j + clen > s->len) clen = 1;
                list_push(seq, vs_bytes(s->data + j, clen));
                j += clen;
            }
        }
        const V *body = map_get(st, "body");
        const List *bl = body->tag == T_LIST ? body->u.l : NULL;
        for (size_t j = 0; j < seq->len; j++) {
            map_insert(locals, strdup(var), seq->items[j]);
            if (bl) {
                Flow f = exec_stmts(rt, bl, locals, argv);
                if (f.kind == F_RETURN) return f;
                if (f.kind == F_BREAK) break;
                if (f.kind == F_CONTINUE) continue;
            }
        }
        return flow_val(v_null());
    }
    return flow_val(v_null());
}

static V eval(Rt *rt, const V *e, Map *locals, List *argv) {
    if (rt->should_exit) return v_null();
    if (e->tag == T_NULL) return v_null();
    char *k = field_cstr(e, "kind");
    if (streq(k, "None")) return v_null();
    if (streq(k, "Int")) {
        Str *s = as_str_val(map_get(e, "value"));
        char *c = xmalloc(s->len + 1); memcpy(c, s->data, s->len); c[s->len] = 0;
        char *end; long long r = strtoll(c, &end, 10);
        return v_int((end == c || *end != 0) ? 0 : (int64_t)r);
    }
    if (streq(k, "Bool")) {
        const V *val = map_get(e, "value");
        if (val->tag == T_BOOL) return v_bool(val->u.b);
        if (val->tag == T_STR) return v_bool(val->u.s->len == 4 && memcmp(val->u.s->data, "true", 4) == 0);
        return v_bool(truthy(val));
    }
    if (streq(k, "Str")) return v_clone(*map_get(e, "value"));
    if (streq(k, "Var")) {
        char *n = field_cstr(e, "name");
        V *r = map_lookup(locals, n);
        return r ? v_clone(*r) : v_null();
    }
    if (streq(k, "Call")) {
        char *callee = field_cstr(e, "callee");
        /* push special-case: O(1) in-place append for `push(localList, x)`. */
        if (streq(callee, "push")) {
            const V *raw = map_get(e, "args");
            if (raw->tag == T_LIST && raw->u.l->len >= 1) {
                const V *first = &raw->u.l->items[0];
                if (streq(field_cstr(first, "kind"), "Var")) {
                    char *n = field_cstr(first, "name");
                    V val = raw->u.l->len > 1 ? eval(rt, &raw->u.l->items[1], locals, argv) : v_null();
                    V *slot = map_lookup(locals, n);
                    if (slot && slot->tag == T_LIST) {
                        List *xs = list_make_mut(slot);
                        list_push(xs, val);
                    } else {
                        List *nl = list_new_cap(1);
                        list_push(nl, val);
                        map_insert(locals, n, vl(nl));
                    }
                    return v_null();
                }
            }
        }
        List *args = list_new_cap(4);
        const V *raw = map_get(e, "args");
        if (raw->tag == T_LIST)
            for (size_t i = 0; i < raw->u.l->len; i++)
                list_push(args, eval(rt, &raw->u.l->items[i], locals, argv));
        return call_fn(rt, callee, args, argv);
    }
    if (streq(k, "Binary")) {
        char *op = field_cstr(e, "op");
        V l = eval(rt, map_get(e, "lhs"), locals, argv);
        V r = eval(rt, map_get(e, "rhs"), locals, argv);
        return eval_bin(op, &l, &r);
    }
    if (streq(k, "Unary")) {
        char *op = field_cstr(e, "op");
        V x = eval(rt, map_get(e, "expr"), locals, argv);
        if (streq(op, "!")) return v_bool(!truthy(&x));
        if (streq(op, "-")) return v_int(-as_i64(&x));
        return x;
    }
    if (streq(k, "Index")) {
        V base = eval(rt, map_get(e, "base"), locals, argv);
        V idx = eval(rt, map_get(e, "index"), locals, argv);
        if (base.tag == T_LIST) {
            int64_t i = as_i64(&idx);
            if (i >= 0 && (size_t)i < base.u.l->len) return v_clone(base.u.l->items[i]);
            return v_null();
        }
        if (base.tag == T_MAP) {
            Str *ks = as_str_val(&idx);
            char *kc = xmalloc(ks->len + 1); memcpy(kc, ks->data, ks->len); kc[ks->len] = 0;
            V *r = map_lookup(base.u.m, kc);
            return r ? v_clone(*r) : v_null();
        }
        if (base.tag == T_STR) {
            int64_t i = as_i64(&idx);
            if (i >= 0 && (size_t)i < base.u.s->len) {
                Str *out = str_new_cap(4);
                push_codepoint(out, (uint32_t)(unsigned char)base.u.s->data[i]);
                return vs_take(out);
            }
            return vs("");
        }
        return v_null();
    }
    if (streq(k, "List")) {
        List *xs = list_new_cap(4);
        const V *els = map_get(e, "elements");
        if (els->tag == T_LIST)
            for (size_t i = 0; i < els->u.l->len; i++)
                list_push(xs, eval(rt, &els->u.l->items[i], locals, argv));
        return vl(xs);
    }
    if (streq(k, "Map")) {
        Map *m = map_new();
        const V *keys = map_get(e, "keys");
        const V *vals = map_get(e, "vals");
        size_t nk = keys->tag == T_LIST ? keys->u.l->len : 0;
        for (size_t i = 0; i < nk; i++) {
            const V *ke = &keys->u.l->items[i];
            Str *ks;
            if (ke->tag == T_STR) ks = ke->u.s;
            else if (streq(field_cstr(ke, "kind"), "Str")) ks = as_str_val(map_get(ke, "value"));
            else ks = as_str_val(ke);
            char *kc = xmalloc(ks->len + 1); memcpy(kc, ks->data, ks->len); kc[ks->len] = 0;
            V vv = (vals->tag == T_LIST && i < vals->u.l->len) ? eval(rt, &vals->u.l->items[i], locals, argv) : v_null();
            map_insert(m, kc, vv);
        }
        return vm(m);
    }
    if (streq(k, "EnumInit")) {
        EnumV *ev = xmalloc(sizeof(EnumV));
        ev->ety = field_cstr(e, "ty");
        ev->variant = field_cstr(e, "variant");
        ev->tuple = NULL;
        ev->nfields = 0;
        ev->fields = NULL;
        char *shape = field_cstr(e, "shape");
        if (streq(shape, "tuple")) {
            List *tup = list_new_cap(2);
            const V *xs = map_get(e, "tuple");
            if (xs->tag == T_LIST)
                for (size_t i = 0; i < xs->u.l->len; i++)
                    list_push(tup, eval(rt, &xs->u.l->items[i], locals, argv));
            ev->tuple = tup;
        } else if (streq(shape, "struct")) {
            const V *fnames = map_get(e, "fnames");
            const V *fexprs = map_get(e, "fexprs");
            size_t fn = fnames->tag == T_LIST ? fnames->u.l->len : 0;
            ev->fields = fn ? xmalloc(sizeof(Pair) * fn) : NULL;
            for (size_t i = 0; i < fn; i++) {
                Str *fns = as_str_val(&fnames->u.l->items[i]);
                char *fname = xmalloc(fns->len + 1); memcpy(fname, fns->data, fns->len); fname[fns->len] = 0;
                V fv = (fexprs->tag == T_LIST && i < fexprs->u.l->len) ? eval(rt, &fexprs->u.l->items[i], locals, argv) : v_null();
                ev->fields[i].key = fname;
                ev->fields[i].val = xmalloc(sizeof(V));
                *ev->fields[i].val = fv;
            }
            ev->nfields = fn;
        }
        V v; v.tag = T_ENUM; v.u.e = ev; return v;
    }
    if (streq(k, "IfExpr")) {
        V cond = eval(rt, map_get(e, "cond"), locals, argv);
        if (truthy(&cond)) {
            const V *body = map_get(e, "then");
            if (body->tag == T_LIST) return eval_block(rt, body->u.l, locals, argv);
        } else {
            const V *body = map_get(e, "else");
            if (body->tag == T_LIST) return eval_block(rt, body->u.l, locals, argv);
        }
        return v_null();
    }
    if (streq(k, "Match")) {
        V scrut = eval(rt, map_get(e, "scrut"), locals, argv);
        const V *arms = map_get(e, "arms");
        if (arms->tag == T_LIST) {
            for (size_t i = 0; i < arms->u.l->len; i++) {
                const V *arm = &arms->u.l->items[i];
                const V *pat = map_get(arm, "pat");
                Map *binds = map_new();
                if (match_pat(pat, &scrut, binds)) {
                    for (size_t j = 0; j < binds->len; j++)
                        map_insert(locals, binds->pairs[j].key, *binds->pairs[j].val);
                    return eval(rt, map_get(arm, "body"), locals, argv);
                }
            }
        }
        die("ANUBIS_SH_MATCH_UNMATCHED");
    }
    if (streq(k, "Range")) return v_clone(*e);
    if (streq(k, "UnsupportedExpr")) return v_null();
    return v_null();
}

/* ------------------------------------------------------------------ */
/* driver                                                             */
/* ------------------------------------------------------------------ */
typedef struct { const char *payload; size_t plen; List *argv; int code; } RunCtx;

static int sh_run(const char *payload, size_t plen, List *argv) {
    V program = parse_json(payload, plen);
    Map *fns = map_new();
    const V *items = map_get(&program, "items");
    if (items->tag == T_LIST) {
        for (size_t i = 0; i < items->u.l->len; i++) {
            const V *it = &items->u.l->items[i];
            if (streq(field_cstr(it, "kind"), "Fn")) {
                char *name = field_cstr(it, "name");
                V copy = v_clone(*it);
                map_insert(fns, name, copy);
            }
        }
    }
    Rt rt;
    rt.program = program;
    rt.fns = fns;
    rt.exit_code = 0;
    rt.should_exit = 0;
    List *noargs = list_new_cap(0);
    call_fn(&rt, "main", noargs, argv);
    return rt.exit_code;
}

static void *run_thread(void *arg) {
    RunCtx *c = (RunCtx *)arg;
    c->code = sh_run(c->payload, c->plen, c->argv);
    return NULL;
}

int main(int argc, char **argv) {
    NULL_V = v_null();
    if (argc < 2) {
        fprintf(stderr, "usage: anubis_sh_c <payload.json> [sh args...]\n");
        return 2;
    }
    /* argv[1] = anubis_sh AST payload (JSON); argv[2..] = SH argv */
    FILE *f = fopen(argv[1], "rb");
    if (!f) { fprintf(stderr, "cannot open payload %s\n", argv[1]); return 2; }
    Str *payload = str_new_cap(1 << 16);
    char buf[65536];
    size_t r;
    while ((r = fread(buf, 1, sizeof buf, f)) > 0) str_push_bytes(payload, buf, r);
    fclose(f);
    char *pc = xmalloc(payload->len + 1);
    memcpy(pc, payload->data, payload->len);
    pc[payload->len] = 0;

    List *sh_argv = list_new_cap((size_t)argc);
    for (int i = 2; i < argc; i++) list_push(sh_argv, vs(argv[i]));

    /* Run on a large stack (256 MB) to match the Rust reference: recursive
     * descent + enum-value recursion can exceed the default main-thread stack. */
    RunCtx ctx = { pc, payload->len, sh_argv, 0 };
    pthread_attr_t attr;
    pthread_attr_init(&attr);
    pthread_attr_setstacksize(&attr, (size_t)256 * 1024 * 1024);
    pthread_t th;
    if (pthread_create(&th, &attr, run_thread, &ctx) != 0) {
        /* fall back to running on the main thread */
        ctx.code = sh_run(pc, payload->len, sh_argv);
    } else {
        pthread_join(th, NULL);
    }
    pthread_attr_destroy(&attr);
    fflush(stdout);
    return ctx.code;
}
