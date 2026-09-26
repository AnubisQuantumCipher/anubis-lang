//! Builtins: what each runtime builtin (`backends/run.rs`, `emit_builtin_call` and the specially
//! lowered names) does to the labels of its arguments, which arguments it applies as callbacks,
//! and which are sources, sinks and stores.
//!
//! Every name the runtime implements has an entry below (a test checks it against `run.rs`); a
//! name with none returns `None` and the caller resolves it as the runtime does (a closure call on
//! a binding of that name). Nothing is given a permissive default: an unlisted builtin that the
//! runtime gains later is a closure call on a local (bottom when there is none), never a clean
//! scalar, and the test fails until it is listed.

use super::eval::{deep_all, index_read, len_label, literal_key, Interp, Path, Store};
use super::value::{FnSet, Lab, ListV, MapV, V};
use crate::frontend::Expr;
use std::collections::BTreeSet;
use std::rc::Rc;

fn scalar(args: &[V]) -> V {
    V::scalar(deep_all(args))
}

/// The elements of an iterable (`anubis_iter`): a list's elements, a string's characters, a
/// map's keys.
fn elems(c: &V) -> V {
    let mut e = V::bottom();
    if let Some(li) = &c.list {
        e = e.join(&li.all);
    }
    if c.scalar {
        e = e.join(&V::scalar(c.lab));
    }
    if let Some(m) = &c.map {
        e = e.join(&V::scalar(m.klab.join(c.lab)));
    }
    if c.top {
        e = e.join(&c.part_of_top());
    }
    e.raise(c.lab)
}

/// What decides how many elements an iterable has (and so how often a callback runs).
fn shape(c: &V) -> Lab {
    len_label(c)
}

/// A list's element at a literal position, or any element.
fn elem_at(c: &V, i: usize) -> V {
    if let Some(li) = &c.list {
        if let Some(items) = &li.items {
            if let Some(x) = items.get(i) {
                let mut out = x.clone();
                if c.scalar || c.map.is_some() || c.top {
                    out = out.join(&elems(c));
                }
                return out.raise(c.lab);
            }
        }
    }
    elems(c)
}

/// A list of `e` whose length is labelled `lab`.
fn list_of(e: V, lab: Lab) -> V {
    V::list_of(e, lab)
}

/// The positions of a value that can only be a list whose positions are known.
fn known_items(c: &V) -> Option<Vec<V>> {
    let only = !c.scalar
        && c.map.is_none()
        && c.structs.is_empty()
        && c.enums.is_empty()
        && c.fns.is_empty()
        && !c.top;
    let items = c.list.as_ref()?.items.as_ref()?;
    only.then(|| items.iter().map(|x| x.raise(c.lab)).collect())
}

/// A non-negative integer literal at argument `i`.
fn literal_count(exprs: &[Expr], i: usize) -> Option<usize> {
    let k = literal_key(exprs.get(i)?)?;
    if k.is_str {
        return None;
    }
    usize::try_from(k.index?).ok()
}

fn arg(args: &[V], i: usize) -> V {
    args.get(i).cloned().unwrap_or_else(|| V::scalar(Lab::PUB))
}

/// Applying a callback: `f(xs)` under the program counter `shape` adds (what decides whether and
/// how often it runs).
fn cb(it: &mut Interp<'_>, f: &V, xs: Vec<V>, shape: Lab, p: &mut Path) -> V {
    let mut sub = p.fork(shape.control());
    let r = it.apply(f, xs, &mut sub);
    p.absorb_lift(&sub);
    r
}

/// A callback the runtime stops calling at a decisive result (`any`, `all`, `find`, `position`,
/// `take_while`, `drop_while`; `sort_by` calls it once per comparison): whether it runs on a later
/// element is decided by its results on the earlier ones (`decides`), so it runs under them too.
fn cb_short(
    it: &mut Interp<'_>,
    f: &V,
    xs: Vec<V>,
    shape: Lab,
    p: &mut Path,
    decides: fn(&V) -> Lab,
) -> V {
    let mut pc = shape.control();
    loop {
        let r = cb(it, f, xs.clone(), pc, p);
        let next = pc.join(decides(&r)).control();
        if next == pc {
            return r;
        }
        pc = next;
    }
}

fn truth(v: &V) -> Lab {
    v.truth()
}

fn deep(v: &V) -> Lab {
    v.deep()
}

/// `name(args)` for a builtin, or `None` when `name` is not one.
pub(crate) fn call<'a>(
    it: &mut Interp<'a>,
    name: &str,
    args: Vec<V>,
    exprs: &'a [Expr],
    p: &mut Path,
) -> Option<V> {
    let a = &args;
    let pc = p.pc();
    let v = match name {
        // ---- output and egress
        "print" | "println" | "eprint" | "eprintln" => {
            it.egress(name, deep_all(a), p);
            V::scalar(Lab::PUB)
        }
        "send" | "network_send" | "connect" | "http_post" | "shell" | "exec" | "system"
        | "target_run" => {
            it.egress(name, deep_all(a), p);
            it.sink(name, deep_all(a));
            V::scalar(Lab::PUB)
        }
        "http_get" => {
            it.egress(name, deep_all(a), p);
            it.sink(name, deep_all(a));
            // The response is not an integrity source in the existing policy.
            V::scalar(Lab::PUB)
        }
        // A panic's message goes to stderr; exit's status is observable; the zkVM journal is
        // public. Either ends the program: the code after it runs only when it did not happen.
        "panic" => {
            let data = deep_all(a);
            if data.secret() {
                it.report(
                    "ANUBIS_SECRET_EXFILTRATION",
                    "secret data reaches `panic` (its message is written to stderr)".into(),
                );
            }
            it.end_program(p);
            V::bottom()
        }
        "exit" => {
            let data = deep_all(a);
            if data.secret() {
                it.report(
                    "ANUBIS_SECRET_EXFILTRATION",
                    "secret data reaches `exit` (the exit status is observable)".into(),
                );
            }
            // `exit()` with no argument is lowered to Int(0) and does not end the program.
            if !a.is_empty() {
                it.end_program(p);
                return Some(V::bottom());
            }
            V::scalar(Lab::PUB)
        }
        "proof_commit_u32" | "proof_commit_bool" | "proof_commit_u64" => {
            let data = a.get(1).map(|v| v.deep()).unwrap_or_default();
            if data.secret() {
                it.report(
                    "ANUBIS_SECRET_EXFILTRATION",
                    format!("secret data reaches `{name}` (the proof journal is public)"),
                );
            }
            arg(a, 1)
        }
        // Files: an integrity sink, and a store that `read_file` and `open` read back.
        "write" | "write_file" | "append_file" | "delete_file" | "remove_file" => {
            it.sink(name, deep_all(a));
            it.store_write(Store::Files, deep_all(a).join(pc));
            V::scalar(Lab::PUB)
        }
        // Integrity sinks that are not confidentiality egress.
        "memcpy" | "sql" => {
            it.sink(name, deep_all(a));
            V::scalar(Lab::PUB)
        }
        "sink" => {
            it.sink(name, deep_all(a));
            arg(a, 0)
        }
        // ---- sources
        "input" | "read_line" | "recv" | "net_recv" => V::scalar(Lab::TNT),
        "env" | "getenv" => V::scalar(Lab::TNT.join(deep_all(a))),
        "read_file" | "open" => {
            V::scalar(Lab::TNT.join(deep_all(a)).join(it.store_read(Store::Files)))
        }
        "secret_source" => arg(a, 0).raise(Lab::SEC),
        "args" => list_of(V::scalar(Lab::PUB), Lab::PUB),
        "time" | "time_now" | "now" | "rand" | "rand_gen" | "random" | "pi" | "e"
        | "crypto_backend" | "keychain_se_probe" => V::scalar(Lab::PUB),
        // A process-global result that minting a non-exportable capability writes.
        "keychain_se_last_bind" => V::scalar(it.store_read(Store::KeychainBind)),
        "proof_input_u32" | "proof_input_u64" | "proof_input_bool" => V::scalar(Lab::PUB),
        // A false condition traps (fail closed, like `assert`); otherwise `true`.
        "proof_assert" => V::scalar(Lab::PUB),
        "cap_acquire_nonexportable" => {
            it.store_write(Store::KeychainBind, deep_all(a).join(pc));
            scalar(a)
        }
        "cap_acquire" | "cap_use" => scalar(a),
        "cap_export" => arg(a, 0),
        "random_bytes" => list_of(V::scalar(Lab::PUB), deep_all(a)),
        "x25519_keygen" | "ed25519_keygen" => {
            list_of(list_of(V::scalar(Lab::PUB), Lab::PUB), Lab::PUB)
        }
        // ---- identity and control of values
        "identity" => arg(a, 0),
        "len" => V::scalar(len_label(&arg(a, 0))),
        "is_empty" => V::scalar(len_label(&arg(a, 0))),
        "type" => V::scalar(arg(a, 0).kind_label()),
        // `char_at(s, i)` is `s.index_get(i)`: a list's element as it is.
        "char_at" => {
            let key = exprs.get(1).and_then(literal_key);
            index_read(&arg(a, 0), &arg(a, 1), key.as_ref())
        }
        // `repeat(xs, n)` of a list repeats its elements; of anything else, its text.
        "repeat" => {
            let (c, n) = (arg(a, 0), arg(a, 1));
            let mut out = V::bottom();
            if c.list.is_some() || c.top {
                out = out.join(&list_of(elems(&c), shape(&c).join(n.deep())));
            }
            let mut rest = c.clone();
            rest.list = None;
            if !rest.is_bottom() {
                out = out.join(&V::scalar(rest.deep().join(n.deep())));
            }
            out
        }
        // Membership: a map's keys, a list's elements, a string's text.
        "contains" => {
            let (c, x) = (arg(a, 0), arg(a, 1));
            let mut l = x.deep();
            if let Some(m) = &c.map {
                l = l.join(c.lab).join(m.klab);
            }
            if c.list.is_some() || c.scalar || c.top {
                l = l.join(c.deep());
            }
            V::scalar(l)
        }
        // ---- pure functions of their arguments' content: a scalar
        "str"
        | "int"
        | "float"
        | "bool"
        | "parse_int"
        | "parse_float"
        | "abs"
        | "sqrt"
        | "floor"
        | "ceil"
        | "round"
        | "sin"
        | "cos"
        | "tan"
        | "asin"
        | "acos"
        | "atan"
        | "exp"
        | "ln"
        | "log10"
        | "log2"
        | "cbrt"
        | "trunc"
        | "sign"
        | "factorial"
        | "pow"
        | "gcd"
        | "atan2"
        | "hypot"
        | "log"
        | "clamp"
        | "upper"
        | "lower"
        | "trim"
        | "ord"
        | "chr"
        | "capitalize"
        | "starts_with"
        | "ends_with"
        | "index_of"
        | "replace"
        | "substr"
        | "pad_start"
        | "pad_end"
        | "join"
        | "sum"
        | "product"
        | "sha256"
        | "sha256_hex"
        | "hash_sha256"
        | "bytes_hex"
        | "to_hex"
        | "hmac_sha256"
        | "hmac_sha256_hex"
        | "hmac_sha256_verify"
        | "ct_eq"
        | "constant_time_eq"
        | "ed25519_verify"
        | "ed25519_public_key"
        | "x25519_public_key"
        | "password_hash"
        | "password_hash_encode"
        | "password_hash_pbkdf2_encode"
        | "password_hash_phc"
        | "password_hash_phc_raw"
        | "password_verify"
        | "password_verify_encoding"
        | "domain_hash"
        | "tuple_hash"
        | "p8"
        | "p16"
        | "p32"
        | "p64"
        | "cyclic"
        | "flat" => scalar(a),
        // min/max return one of their arguments (a single list argument is spread); which one is
        // decided by every element's value.
        "min" | "max" => {
            let spread = a.len() == 1 && (a[0].list.is_some() || a[0].top);
            let items = if spread {
                let c = &a[0];
                let mut e = elems(c);
                let mut rest = c.clone();
                rest.list = None;
                rest.top = false;
                if !rest.is_bottom() {
                    e = e.join(&rest);
                }
                e
            } else {
                a.iter().fold(V::bottom(), |acc, x| acc.join(x))
            };
            // One candidate (a single non-list argument, or a one-element list literal): no choice.
            let candidates = if a.len() == 1 {
                match (&a[0].list, a[0].top) {
                    (_, true) => usize::MAX,
                    (Some(li), _) => li.items.as_ref().map(|i| i.len()).unwrap_or(usize::MAX),
                    (None, _) => 1,
                }
            } else {
                a.len()
            };
            let chooser = if candidates <= 1 {
                Lab::PUB
            } else {
                deep_all(a)
            };
            items.raise(chooser)
        }
        "parse_int_opt" | "parse_float_opt" => {
            let l = deep_all(a);
            let mut v = V::bottom();
            v.enums.insert(
                (Rc::from("Option"), Rc::from("Some")),
                Rc::new(super::value::EnumV {
                    fields: vec![V::scalar(l)],
                    names: vec![],
                }),
            );
            v.enums.insert(
                (Rc::from("Option"), Rc::from("None")),
                Rc::new(super::value::EnumV {
                    fields: vec![],
                    names: vec![],
                }),
            );
            v.raise(l)
        }
        // Byte-list results of cryptographic functions.
        "sha256_bytes"
        | "hmac_sha256_bytes"
        | "aead_nonce_from_counter"
        | "aead_seal"
        | "chacha20_poly1305_seal"
        | "aead_open"
        | "chacha20_poly1305_open"
        | "x25519_shared"
        | "ed25519_sign"
        | "hkdf_sha256"
        | "pbkdf2_hmac_sha256"
        | "argon2id_hash" => {
            let l = deep_all(a);
            list_of(V::scalar(l), l)
        }
        "hybrid_seal" | "hybrid_open" => {
            let l = deep_all(a);
            list_of(list_of(V::scalar(l), l), l)
        }
        // ---- strings to lists
        "chars" | "words" | "lines" | "split" => {
            let l = deep_all(a);
            list_of(V::scalar(l), l)
        }
        // ---- lists
        "reverse" => {
            let c = arg(a, 0);
            match &c.list {
                Some(li) => {
                    let mut li2 = (**li).clone();
                    if let Some(items) = &mut li2.items {
                        items.reverse();
                    }
                    let mut out = c.clone();
                    out.list = Some(Rc::new(li2));
                    if c.top {
                        out = out.join(&c.part_of_top());
                    }
                    out
                }
                None => {
                    // A string reverses to a string; anything else traps.
                    let mut out = V::bottom();
                    if c.scalar {
                        out = out.join(&V::scalar(c.deep()));
                    }
                    if c.top {
                        out = out.join(&c.part_of_top());
                    }
                    out
                }
            }
        }
        // Sorting keeps the length; the element at each position is decided by comparing every
        // element's content.
        "sort" => {
            let c = arg(a, 0);
            let e = elems(&c);
            let d = e.deep();
            list_of(e.raise(d), shape(&c))
        }
        // Deduplication's length, and which element is at each position, reveal which elements
        // are equal.
        "unique" => {
            let c = arg(a, 0);
            let e = elems(&c);
            let d = e.deep();
            list_of(e.raise(d), shape(&c).join(d))
        }
        "take" | "drop" => {
            let c = arg(a, 0);
            let rest = deep_all(&a[1.min(a.len())..]);
            // A literal count over known positions keeps them.
            if let (Some(items), Some(n)) = (known_items(&c), literal_count(exprs, 1)) {
                let n = n.min(items.len());
                let kept = if name == "take" {
                    items[..n].to_vec()
                } else {
                    items[n..].to_vec()
                };
                return Some(V::list(kept, c.lab));
            }
            list_of(elems(&c).raise(rest), shape(&c).join(rest))
        }
        // slice: a list's positions, or a string's substring.
        "slice" => {
            let c = arg(a, 0);
            let rest = deep_all(&a[1.min(a.len())..]);
            let mut out = V::bottom();
            if let Some(li) = &c.list {
                out = out.join(&list_of(li.all.raise(c.lab).raise(rest), c.lab.join(rest)));
            }
            if c.scalar {
                out = out.join(&V::scalar(c.deep().join(rest)));
            }
            if c.top {
                out = out.join(&c.part_of_top().raise(rest));
            }
            out
        }
        "first" => elem_at(&arg(a, 0), 0),
        "last" => {
            let c = arg(a, 0);
            let at = c
                .list
                .as_ref()
                .and_then(|li| li.items.as_ref().map(|i| i.len()))
                .filter(|n| *n > 0);
            match at {
                Some(n) => elem_at(&c, n - 1),
                None => elems(&c),
            }
        }
        "concat" => {
            let (x, y) = (arg(a, 0), arg(a, 1));
            list_of(elems(&x).join(&elems(&y)), shape(&x).join(shape(&y)))
        }
        "flatten" => {
            let c = arg(a, 0);
            let inner = elems(&c);
            list_of(elems(&inner), shape(&c).join(shape(&inner)))
        }
        "enumerate" => {
            let c = arg(a, 0);
            let s = shape(&c);
            let pair = V::list(vec![V::scalar(s), elems(&c)], Lab::PUB);
            list_of(pair, s)
        }
        "zip" => {
            let (x, y) = (arg(a, 0), arg(a, 1));
            let l = shape(&x).join(shape(&y));
            list_of(V::list(vec![elems(&x), elems(&y)], Lab::PUB), l)
        }
        "chunk" | "window" => {
            let c = arg(a, 0);
            // A literal size over known positions keeps them.
            if let (Some(items), Some(n)) = (known_items(&c), literal_count(exprs, 1)) {
                if n > 0 {
                    let parts: Vec<V> = if name == "chunk" {
                        items
                            .chunks(n)
                            .map(|ch| V::list(ch.to_vec(), c.lab))
                            .collect()
                    } else if items.len() < n {
                        Vec::new()
                    } else {
                        items
                            .windows(n)
                            .map(|w| V::list(w.to_vec(), c.lab))
                            .collect()
                    };
                    return Some(V::list(parts, c.lab));
                }
            }
            let l = shape(&c).join(arg(a, 1).deep());
            list_of(list_of(elems(&c), l), l)
        }
        "range" => {
            let l = deep_all(a);
            list_of(V::scalar(l), l)
        }
        // ---- maps
        "keys" => {
            let m = arg(a, 0);
            let s = shape(&m);
            let mut e = V::scalar(s);
            if m.top {
                e = e.join(&m.part_of_top());
            }
            list_of(e, s)
        }
        "values" => {
            let m = arg(a, 0);
            let mut all = V::bottom();
            if let Some(x) = &m.map {
                all = all.join(&x.other);
                for v in x.known.values() {
                    all = all.join(v);
                }
            }
            if m.top {
                all = all.join(&m.part_of_top());
            }
            let s = shape(&m);
            list_of(all.raise(s), s)
        }
        "entries" => {
            let m = arg(a, 0);
            let mut all = V::bottom();
            if let Some(x) = &m.map {
                all = all.join(&x.other);
                for v in x.known.values() {
                    all = all.join(v);
                }
            }
            if m.top {
                all = all.join(&m.part_of_top());
            }
            let s = shape(&m);
            let pair = V::list(vec![V::scalar(s), all.raise(s)], Lab::PUB);
            list_of(pair, s)
        }
        "merge" => merge(&arg(a, 0), &arg(a, 1)),
        // Whether a key is present: the key set, not the values.
        // (`anubis_has_key` takes a map; any other kind traps.)
        "has_key" => {
            let (m, k) = (arg(a, 0), arg(a, 1));
            let mut l = m.lab.join(k.deep());
            if let Some(x) = &m.map {
                l = l.join(x.klab);
            }
            V::scalar(l)
        }
        "get" => {
            // get(m, k, default): a map's value at k, a list's element or a string's character at
            // position k, else the default (any other kind, structs included, gives the default).
            let (m, k, dflt) = (arg(a, 0), arg(a, 1), arg(a, 2));
            let key = exprs.get(1).and_then(literal_key);
            let mut found = V::bottom();
            if let Some(x) = &m.map {
                let mut only = V::bottom();
                only.map = Some(x.clone());
                only.lab = m.lab;
                found = found.join(&index_read(&only, &k, key.as_ref()));
            }
            if let Some(li) = &m.list {
                let mut only = V::bottom();
                only.list = Some(li.clone());
                only.lab = m.lab;
                found = found.join(&index_read(&only, &k, key.as_ref()));
            }
            if m.scalar {
                found = found.join(&V::scalar(m.lab));
            }
            if m.top {
                found = found.join(&m.part_of_top());
            }
            // Whether the default is taken depends on the key and the container's shape.
            let mut decided = k.deep();
            if m.map.is_some() || m.list.is_some() || m.scalar || m.top {
                decided = decided.join(len_label(&m));
            }
            found.join(&dflt).raise(decided)
        }
        // ---- higher-order: the runtime applies the callback to each element
        "map" => {
            let (c, f) = (arg(a, 0), arg(a, 1));
            let s = shape(&c);
            let r = cb(it, &f, vec![elems(&c)], s, p);
            list_of(r, s)
        }
        "flat_map" => {
            let (c, f) = (arg(a, 0), arg(a, 1));
            let s = shape(&c);
            let r = cb(it, &f, vec![elems(&c)], s, p);
            list_of(elems(&r), s.join(shape(&r)))
        }
        "filter" | "partition" | "count" => {
            let (c, f) = (arg(a, 0), arg(a, 1));
            let s = shape(&c);
            let r = cb(it, &f, vec![elems(&c)], s, p);
            let t = truth(&r);
            let kept = list_of(elems(&c).raise(t), s.join(t));
            match name {
                "filter" => kept,
                "partition" => V::list(vec![kept.clone(), kept], Lab::PUB),
                _ => V::scalar(s.join(t)),
            }
        }
        "take_while" | "drop_while" => {
            let (c, f) = (arg(a, 0), arg(a, 1));
            let s = shape(&c);
            let r = cb_short(it, &f, vec![elems(&c)], s, p, truth);
            let t = truth(&r);
            list_of(elems(&c).raise(t), s.join(t))
        }
        "each" => {
            let (c, f) = (arg(a, 0), arg(a, 1));
            let s = shape(&c);
            let _ = cb(it, &f, vec![elems(&c)], s, p);
            V::scalar(Lab::PUB)
        }
        "find" => {
            let (c, f) = (arg(a, 0), arg(a, 1));
            let s = shape(&c);
            let r = cb_short(it, &f, vec![elems(&c)], s, p, truth);
            elems(&c).raise(truth(&r).join(s))
        }
        // The key function is called a number of times fixed by the length (`Iterator::min_by`).
        "min_by" | "max_by" => {
            let (c, f) = (arg(a, 0), arg(a, 1));
            let s = shape(&c);
            let r = cb(it, &f, vec![elems(&c)], s, p);
            elems(&c).raise(r.deep().join(s))
        }
        "any" | "all" | "position" => {
            let (c, f) = (arg(a, 0), arg(a, 1));
            let s = shape(&c);
            let r = cb_short(it, &f, vec![elems(&c)], s, p, truth);
            V::scalar(truth(&r).join(s))
        }
        // `sort_by` calls the key function per comparison, and which comparisons run depends on
        // the keys; the length is kept.
        "sort_by" => {
            let (c, f) = (arg(a, 0), arg(a, 1));
            let s = shape(&c);
            let r = cb_short(it, &f, vec![elems(&c)], s, p, deep);
            list_of(elems(&c).raise(r.deep()), s)
        }
        "map_values" => {
            let (m, f) = (arg(a, 0), arg(a, 1));
            let mut out = V::bottom();
            if let Some(x) = &m.map {
                let mut x2 = (**x).clone();
                let s = m.lab.join(x.klab);
                for v in x2.known.values_mut() {
                    *v = cb(it, &f, vec![v.clone()], s, p);
                }
                x2.other = cb(it, &f, vec![x.other.clone()], s, p);
                out = out.join(&V {
                    lab: m.lab,
                    map: Some(Rc::new(x2)),
                    ..V::default()
                });
            }
            if m.top {
                // Any map: the callback on any of its values.
                let s = m.lab.join(m.tlab);
                let r = cb(it, &f, vec![m.part_of_top()], s, p);
                out = out.join(&V {
                    lab: s,
                    map: Some(Rc::new(MapV {
                        known: Default::default(),
                        other: r,
                        klab: s,
                        maybe: BTreeSet::new(),
                    })),
                    ..V::default()
                });
            }
            out
        }
        "times" => {
            let (n, f) = (arg(a, 0), arg(a, 1));
            let r = cb(it, &f, vec![V::scalar(n.deep())], n.deep(), p);
            list_of(r, n.deep())
        }
        "apply" => {
            // apply(f, list): f(*list) (every element, as many as there are), or f(x) for a
            // non-list.
            let (f, xs) = (arg(a, 0), arg(a, 1));
            let mut out = V::bottom();
            let unknown_len = match &xs.list {
                Some(li) => li.items.is_none(),
                None => false,
            } || xs.top;
            if let (Some(li), false) = (&xs.list, unknown_len) {
                if let Some(items) = &li.items {
                    let spread: Vec<V> = items.iter().map(|x| x.raise(xs.lab)).collect();
                    out = out.join(&cb(it, &f, spread, xs.lab, p));
                }
            }
            if unknown_len {
                let n = it.max_arity(&f);
                let e = elems(&xs).join(&V::scalar(Lab::PUB));
                let s = shape(&xs);
                out = out.join(&cb(it, &f, vec![e.raise(s); n], s, p));
            }
            let mut other = xs.clone();
            other.list = None;
            other.top = false;
            if !other.is_bottom() {
                out = out.join(&cb(it, &f, vec![other], xs.kind_label(), p));
            }
            out
        }
        "call" => {
            let f = arg(a, 0);
            let rest: Vec<V> = a.iter().skip(1).cloned().collect();
            cb(it, &f, rest, Lab::PUB, p)
        }
        "reduce" => reduce(it, a, p),
        "compose" => {
            let mut fs = FnSet::default();
            fs.composed.insert((Rc::new(arg(a, 0)), Rc::new(arg(a, 1))));
            V::func(fs, Lab::PUB)
        }
        // ---- mutators given a non-variable first argument: a lowering error at runtime.
        "push" | "pop" | "insert" | "remove" => V::bottom(),
        _ => return None,
    };
    Some(v)
}

/// `merge(a, b)` (`anubis_merge`): `a`'s entries with `b`'s added, `b` winning on a shared key.
fn merge(x: &V, y: &V) -> V {
    let (Some(a), Some(b)) = (&x.map, &y.map) else {
        return x.join(y);
    };
    let mut known = b.known.clone();
    let mut maybe = b.maybe.clone();
    for (k, v) in &a.known {
        if !b.known.contains_key(k) {
            // `b` may have it under a computed key.
            known.insert(k.clone(), v.join(&b.other));
            if a.maybe.contains(k) {
                maybe.insert(k.clone());
            }
        }
    }
    let mut out = V {
        lab: x.lab.join(y.lab),
        map: Some(Rc::new(MapV {
            known,
            other: a.other.join(&b.other),
            klab: a.klab.join(b.klab),
            maybe,
        })),
        ..V::default()
    };
    // Other kinds of either argument: the runtime traps.
    if x.top || y.top {
        out = out.join(&x.join(y));
    }
    out
}

/// `reduce(list, f)` seeds with the first element; `reduce(list, a, b)` folds with whichever of
/// `a` and `b` is a closure at runtime and seeds with the other (which one that is, is decided by
/// their kinds).
fn reduce<'a>(it: &mut Interp<'a>, a: &[V], p: &mut Path) -> V {
    let c = arg(a, 0);
    let e = elems(&c);
    let s = shape(&c);
    let mut orders: Vec<(V, V)> = Vec::new();
    if a.len() <= 2 {
        orders.push((arg(a, 1), e.clone()));
    } else {
        let (x, y) = (arg(a, 1), arg(a, 2));
        if !x.fns.is_empty() || x.top {
            orders.push((x.clone(), y.clone()));
        }
        let x_not_closure = x.scalar
            || x.list.is_some()
            || x.map.is_some()
            || !x.structs.is_empty()
            || !x.enums.is_empty()
            || x.top;
        if x_not_closure && (!y.fns.is_empty() || y.top) {
            orders.push((y.clone(), x.clone()));
        }
    }
    let decided = if orders.len() > 1 {
        arg(a, 1)
            .kind_label()
            .join(arg(a, 2).kind_label())
            .join(arg(a, 1).lab)
    } else {
        Lab::PUB
    };
    let pc = s.join(decided);
    let mut out = V::bottom();
    for (f, seed) in orders {
        // acc₀ = seed; accₙ₊₁ = f(accₙ, x): iterate to a fixpoint.
        let mut acc = seed.raise(s);
        let mut rounds = 0;
        loop {
            rounds += 1;
            let r = cb(it, &f, vec![acc.clone(), e.clone()], pc, p);
            if r.leq(&acc) || it.exhausted() {
                break;
            }
            if rounds > 60 {
                it.give_up("a reduce did not settle");
                break;
            }
            acc = if rounds > 12 {
                it.widen_coarse(&acc.join(&r))
            } else {
                it.widen(&acc.join(&r))
            };
        }
        out = out.join(&acc);
    }
    out.raise(pc)
}

/// `push`, `pop`, `insert`, `remove` on a variable (`&mut var`), in statement or expression
/// position. Returns what the call yields: push the container, pop and remove the element, insert
/// Int(0).
pub(crate) fn mutate<'a>(it: &mut Interp<'a>, name: &str, args: &'a [Expr], p: &mut Path) -> V {
    let Some(Expr::Var(var)) = args.first() else {
        return V::bottom();
    };
    let rest: Vec<V> = it.eval_args(&args[1..], p);
    let Some(st) = &p.st else {
        return V::bottom();
    };
    let pc = st.pc().control();
    let Some(cur) = st.lookup_pub(var).cloned() else {
        return V::bottom();
    };
    let (updated, yielded) = match name {
        "push" => {
            let v = arg(&rest, 0).raise(pc);
            let mut out = cur.clone();
            match &cur.list {
                Some(li) => {
                    out.list = Some(Rc::new(match li.items.clone() {
                        Some(mut i) => {
                            i.push(v.clone());
                            ListV::of_items(i)
                        }
                        None => ListV {
                            items: None,
                            all: li.all.join(&v),
                        },
                    }));
                    out.lab = out.lab.join(pc);
                }
                None if cur.top => {}
                None => {}
            }
            if cur.top {
                out.store_in_top(None, &v);
                out.lab = out.lab.join(pc);
                out.absorb_fns(&v);
            }
            (out.clone(), out)
        }
        "pop" => {
            let mut out = cur.clone();
            let mut got = V::bottom();
            if let Some(li) = &cur.list {
                let li2 = match li.items.clone() {
                    Some(mut items) if !items.is_empty() => {
                        got = items.pop().unwrap_or_default();
                        ListV::of_items(items)
                    }
                    _ => {
                        got = li.all.clone();
                        ListV {
                            items: None,
                            all: li.all.clone(),
                        }
                    }
                };
                out.list = Some(Rc::new(li2));
                out.lab = out.lab.join(pc);
            }
            if cur.top {
                got = got.join(&cur.part_of_top());
                out.lab = out.lab.join(pc);
            }
            (out, got.raise(cur.lab))
        }
        "insert" => {
            let (k, v) = (arg(&rest, 0), arg(&rest, 1).raise(pc));
            let mut out = cur.clone();
            if let Some(li) = &cur.list {
                out.list = Some(Rc::new(ListV {
                    items: None,
                    all: li.all.join(&v.raise(k.deep())),
                }));
                out.lab = out.lab.join(pc).join(k.deep());
            }
            if let Some(m) = &cur.map {
                let mut m2: MapV = (**m).clone();
                match args.get(1).and_then(literal_key) {
                    Some(key) if k.deep() == Lab::PUB => {
                        if !m.known.contains_key(&key.display) || m.maybe.contains(&key.display) {
                            out.lab = out.lab.join(pc);
                        }
                        m2.maybe.remove(&key.display);
                        m2.known.insert(key.display, v.clone());
                    }
                    _ => {
                        m2.other = m2.other.join(&v.raise(k.deep()));
                        m2.klab = m2.klab.join(k.deep());
                        for slot in m2.known.values_mut() {
                            *slot = slot.join(&v);
                        }
                        out.lab = out.lab.join(pc);
                    }
                }
                out.map = Some(Rc::new(m2));
            }
            if cur.top {
                out.tlab = out.tlab.join(k.deep());
                out.store_in_top(None, &v);
                out.lab = out.lab.join(k.deep()).join(pc);
                out.absorb_fns(&v);
            }
            (out, V::scalar(Lab::PUB))
        }
        _ => {
            // remove(var, k): the element or value removed.
            let k = arg(&rest, 0);
            let key = args.get(1).and_then(literal_key);
            let got = index_read(&cur, &k, key.as_ref());
            let mut out = cur.clone();
            if let Some(li) = &cur.list {
                out.list = Some(Rc::new(ListV {
                    items: None,
                    all: li.all.clone(),
                }));
                out.lab = out.lab.join(pc).join(k.deep());
            }
            if let Some(m) = &cur.map {
                let mut m2: MapV = (**m).clone();
                if let Some(key) = &key {
                    if k.deep() == Lab::PUB {
                        m2.known.remove(&key.display);
                        m2.maybe.remove(&key.display);
                    }
                }
                m2.klab = m2.klab.join(pc).join(k.deep());
                out.lab = out.lab.join(pc);
                out.map = Some(Rc::new(m2));
            }
            if cur.top {
                out.lab = out.lab.join(pc).join(k.deep());
            }
            (out, got)
        }
    };
    if let Some(st) = &mut p.st {
        st.assign_pub(var, updated);
    }
    yielded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::run::is_builtin_name;

    /// Names the interpreter handles before the builtin table (`Interp::eval_call`,
    /// `eval_stmt_expr`), exactly as the runtime lowers them specially.
    const HANDLED_BY_EVAL: &[&str] = &[
        "print", "println", "eprint", "eprintln", "return", "break", "continue", "len", "push",
        "pop", "insert", "remove",
    ];

    /// Keywords the parser turns into their own expressions (`Expr::Assert`, `Expr::Declassify`,
    /// …): never a call by name.
    const KEYWORDS: &[&str] = &["assert", "assume", "declassify", "symbolic", "taint_source"];

    /// Every builtin the runtime implements (every quoted identifier in `run.rs` that
    /// `is_builtin_name` accepts) has an entry in this table: none silently falls through to a
    /// closure call (bottom, which would drop its arguments' labels).
    #[test]
    fn every_runtime_builtin_has_an_entry() {
        let src = include_str!("../../backends/run.rs");
        let mut names = BTreeSet::new();
        let mut rest = src;
        while let Some(i) = rest.find('"') {
            rest = &rest[i + 1..];
            let Some(j) = rest.find('"') else { break };
            let lit = &rest[..j];
            rest = &rest[j + 1..];
            let ident = !lit.is_empty()
                && lit.chars().all(|c| c == '_' || c.is_ascii_alphanumeric())
                && !lit.chars().next().is_some_and(|c| c.is_ascii_digit());
            if ident && is_builtin_name(lit) {
                names.insert(lit.to_string());
            }
        }
        assert!(
            names.len() > 150,
            "found only {} builtin names",
            names.len()
        );
        let items: &'static [crate::frontend::Item] = &[];
        let mut missing = Vec::new();
        for n in &names {
            if HANDLED_BY_EVAL.contains(&n.as_str()) || KEYWORDS.contains(&n.as_str()) {
                continue;
            }
            let mut it = Interp::new(items);
            let mut p = Path::test_live();
            let args = vec![V::scalar(Lab::PUB); 3];
            if call(&mut it, n, args, &[], &mut p).is_none() {
                missing.push(n.clone());
            }
        }
        assert!(missing.is_empty(), "builtins with no entry: {missing:?}");
    }
}
