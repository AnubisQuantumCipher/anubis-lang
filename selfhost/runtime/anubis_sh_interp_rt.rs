#![allow(dead_code)]
// Anubis-SH executable backend — AST interpreter (no deps).
// Stage-1+ packages: this file + `const PAYLOAD: &str = "...";` + main calling sh_run.
//
// Covers the Anubis-SH self-host subset AND the full executable language surface
// (enums, match, if-expressions, for-in over collections) so that a stage package
// runs the full language identically to the Rust host on the `anubis run` corpus.
//
// Performance: the three heap value kinds (Str/List/Map) are Rc-wrapped so that
// reading a variable or passing a value as an argument is an O(1) refcount bump,
// not a deep clone. Mutation is copy-on-write via Rc::make_mut. Without this, a
// self-hosting compiler that threads the 236 KB source string and the 30k-token
// list through millions of calls is quadratic. String builtins index by byte
// (ASCII source, SUBSET.md) for O(1) access.
//
// Value/host fidelity notes (honest, verified against `anubis run`):
//   * Maps iterate keys in sorted order (BTreeMap). The host uses insertion order.
//     This is observable only for programs whose *output* depends on map-key
//     iteration order; no program in the verified corpus does.
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::rc::Rc;

#[derive(Clone, Debug)]
enum V {
    Null,
    Bool(bool),
    Int(i64),
    Str(Rc<String>),
    List(Rc<Vec<V>>),
    Map(Rc<BTreeMap<String, V>>),
    // Algebraic value: (enum type, variant, positional/tuple fields, named/struct fields).
    Enum(String, String, Vec<V>, Vec<(String, V)>),
}

// Cheap constructors.
fn vs(s: String) -> V {
    V::Str(Rc::new(s))
}
fn vl(xs: Vec<V>) -> V {
    V::List(Rc::new(xs))
}
fn vm(m: BTreeMap<String, V>) -> V {
    V::Map(Rc::new(m))
}

impl V {
    fn display(&self) -> String {
        match self {
            V::Null => String::new(),
            V::Bool(b) => b.to_string(),
            V::Int(n) => n.to_string(),
            V::Str(s) => (**s).clone(),
            V::List(xs) => {
                let p: Vec<_> = xs.iter().map(|x| x.display()).collect();
                format!("[{}]", p.join(", "))
            }
            V::Map(_) => "{...}".into(),
            V::Enum(_, variant, tuple, fields) => {
                if !tuple.is_empty() {
                    let p: Vec<_> = tuple.iter().map(|x| x.display()).collect();
                    format!("{}({})", variant, p.join(", "))
                } else if !fields.is_empty() {
                    let p: Vec<_> = fields
                        .iter()
                        .map(|(k, v)| format!("{}: {}", k, v.display()))
                        .collect();
                    format!("{} {{ {} }}", variant, p.join(", "))
                } else {
                    variant.clone()
                }
            }
        }
    }
    fn truthy(&self) -> bool {
        match self {
            V::Null => false,
            V::Bool(b) => *b,
            V::Int(n) => *n != 0,
            V::Str(s) => !s.is_empty(),
            V::List(xs) => !xs.is_empty(),
            V::Map(m) => !m.is_empty(),
            V::Enum(..) => true,
        }
    }
    fn as_i64(&self) -> i64 {
        match self {
            V::Int(n) => *n,
            V::Bool(b) => {
                if *b {
                    1
                } else {
                    0
                }
            }
            V::Str(s) => s.parse().unwrap_or(0),
            _ => 0,
        }
    }
}

// ---- minimal JSON parser for SH jast output ----
struct Jp<'a> {
    s: &'a [u8],
    i: usize,
}
impl<'a> Jp<'a> {
    fn new(s: &'a str) -> Self {
        Self {
            s: s.as_bytes(),
            i: 0,
        }
    }
    fn skip_ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }
    fn peek(&mut self) -> u8 {
        self.skip_ws();
        if self.i < self.s.len() {
            self.s[self.i]
        } else {
            0
        }
    }
    fn bump(&mut self) -> u8 {
        self.skip_ws();
        let c = self.s[self.i];
        self.i += 1;
        c
    }
    fn parse(&mut self) -> V {
        match self.peek() {
            b'n' => {
                self.i += 4;
                V::Null
            }
            b't' => {
                self.i += 4;
                V::Bool(true)
            }
            b'f' => {
                self.i += 5;
                V::Bool(false)
            }
            b'"' => vs(self.string()),
            b'[' => self.array(),
            b'{' => self.object(),
            b'-' | b'0'..=b'9' => self.number(),
            _ => panic!("json at {}", self.i),
        }
    }
    fn string(&mut self) -> String {
        assert_eq!(self.bump(), b'"');
        let mut out = String::new();
        while self.i < self.s.len() {
            let c = self.s[self.i];
            self.i += 1;
            if c == b'"' {
                break;
            }
            if c == b'\\' {
                let e = self.s[self.i];
                self.i += 1;
                match e {
                    b'n' => out.push('\n'),
                    b't' => out.push('\t'),
                    b'r' => out.push('\r'),
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'/' => out.push('/'),
                    b'u' => {
                        let hex =
                            std::str::from_utf8(&self.s[self.i..self.i + 4]).unwrap_or("0000");
                        self.i += 4;
                        if let Ok(v) = u32::from_str_radix(hex, 16) {
                            if let Some(ch) = char::from_u32(v) {
                                out.push(ch);
                            }
                        }
                    }
                    _ => out.push(e as char),
                }
            } else {
                out.push(c as char);
            }
        }
        out
    }
    fn number(&mut self) -> V {
        self.skip_ws();
        let start = self.i;
        if self.s[self.i] == b'-' {
            self.i += 1;
        }
        while self.i < self.s.len() && self.s[self.i].is_ascii_digit() {
            self.i += 1;
        }
        let s = std::str::from_utf8(&self.s[start..self.i]).unwrap();
        V::Int(s.parse().unwrap_or(0))
    }
    fn array(&mut self) -> V {
        assert_eq!(self.bump(), b'[');
        let mut xs = Vec::new();
        if self.peek() == b']' {
            self.bump();
            return vl(xs);
        }
        loop {
            xs.push(self.parse());
            if self.peek() == b',' {
                self.bump();
                continue;
            }
            break;
        }
        assert_eq!(self.bump(), b']');
        vl(xs)
    }
    fn object(&mut self) -> V {
        assert_eq!(self.bump(), b'{');
        let mut m = BTreeMap::new();
        if self.peek() == b'}' {
            self.bump();
            return vm(m);
        }
        loop {
            let k = self.string();
            assert_eq!(self.bump(), b':');
            let v = self.parse();
            m.insert(k, v);
            if self.peek() == b',' {
                self.bump();
                continue;
            }
            break;
        }
        assert_eq!(self.bump(), b'}');
        vm(m)
    }
}

fn parse_json(s: &str) -> V {
    Jp::new(s).parse()
}

fn map_get<'a>(m: &'a V, k: &str) -> &'a V {
    match m {
        V::Map(map) => map.get(k).unwrap_or(&V::Null),
        _ => &V::Null,
    }
}
fn as_str_val(v: &V) -> String {
    match v {
        V::Str(s) => (**s).clone(),
        other => other.display(),
    }
}
// Borrow a string value without cloning (used on hot paths like char_at/substr).
fn str_ref(v: &V) -> Option<&str> {
    match v {
        V::Str(s) => Some(s.as_str()),
        _ => None,
    }
}
fn as_bool(v: &V) -> bool {
    v.truthy()
}

struct Rt {
    program: V,
    // Function table built once (H3): each fn wrapped in Rc so calls clone a pointer,
    // not a deep copy of the whole function body.
    fns: BTreeMap<String, Rc<V>>,
    exit_code: i32,
    should_exit: bool,
}

impl Rt {
    fn call(&mut self, name: &str, args: Vec<V>, argv: &mut Vec<String>) -> V {
        if self.should_exit {
            return V::Null;
        }
        // builtins
        match name {
            "print" | "println" => {
                if let Some(a) = args.first() {
                    println!("{}", a.display());
                }
                return V::Null;
            }
            "len" => {
                let n = match args.first().unwrap_or(&V::Null) {
                    V::Str(s) => s.len() as i64,
                    V::List(xs) => xs.len() as i64,
                    V::Map(m) => m.len() as i64,
                    _ => 0,
                };
                return V::Int(n);
            }
            "char_at" => {
                // Byte index (ASCII source) → O(1), by reference (no src clone).
                let i = args.get(1).map(|x| x.as_i64()).unwrap_or(0);
                if i >= 0 {
                    if let Some(s) = args.first().and_then(str_ref) {
                        if let Some(b) = s.as_bytes().get(i as usize) {
                            return vs((*b as char).to_string());
                        }
                    }
                }
                return vs(String::new());
            }
            "ord" => {
                let b = args.first().and_then(str_ref).and_then(|s| s.as_bytes().first().copied());
                return V::Int(b.map(|b| b as i64).unwrap_or(0));
            }
            "chr" => {
                let n = args.first().map(|x| x.as_i64()).unwrap_or(0) as u32;
                return vs(char::from_u32(n).unwrap_or('\0').to_string());
            }
            "substr" => {
                let st = args.get(1).map(|x| x.as_i64()).unwrap_or(0).max(0) as usize;
                let ln = args.get(2).map(|x| x.as_i64()).unwrap_or(0).max(0) as usize;
                if let Some(s) = args.first().and_then(str_ref) {
                    let bytes = s.as_bytes();
                    let start = st.min(bytes.len());
                    let end = st.saturating_add(ln).min(bytes.len());
                    return vs(String::from_utf8_lossy(&bytes[start..end]).into_owned());
                }
                return vs(String::new());
            }
            "index_of" => {
                let sub = as_str_val(args.get(1).unwrap_or(&V::Null));
                if let Some(s) = args.first().and_then(str_ref) {
                    return V::Int(s.find(&sub).map(|i| i as i64).unwrap_or(-1));
                }
                return V::Int(-1);
            }
            "split" => {
                let s = as_str_val(args.first().unwrap_or(&V::Null));
                let sep = as_str_val(args.get(1).unwrap_or(&V::Null));
                let parts: Vec<V> = if sep.is_empty() {
                    s.chars().map(|c| vs(c.to_string())).collect()
                } else {
                    s.split(&sep).map(|p| vs(p.to_string())).collect()
                };
                return vl(parts);
            }
            "push" => {
                let mut xs = match args.first() {
                    Some(V::List(v)) => (**v).clone(),
                    _ => Vec::new(),
                };
                if let Some(v) = args.get(1) {
                    xs.push(v.clone());
                }
                return vl(xs);
            }
            "get" => {
                let m = args.first().unwrap_or(&V::Null);
                let k = as_str_val(args.get(1).unwrap_or(&V::Null));
                let def = args.get(2).cloned().unwrap_or(V::Null);
                return match m {
                    V::Map(map) => map.get(&k).cloned().unwrap_or(def),
                    _ => def,
                };
            }
            "has_key" => {
                let m = args.first().unwrap_or(&V::Null);
                let k = as_str_val(args.get(1).unwrap_or(&V::Null));
                return match m {
                    V::Map(map) => V::Bool(map.contains_key(&k)),
                    _ => V::Bool(false),
                };
            }
            "keys" => {
                return match args.first() {
                    Some(V::Map(m)) => vl(m.keys().map(|k| vs(k.clone())).collect()),
                    _ => vl(vec![]),
                };
            }
            "values" => {
                return match args.first() {
                    Some(V::Map(m)) => vl(m.values().cloned().collect()),
                    _ => vl(vec![]),
                };
            }
            "read_file" => {
                let p = as_str_val(args.first().unwrap_or(&V::Null));
                return match fs::read_to_string(&p) {
                    Ok(s) => vs(s),
                    Err(e) => panic!("read_file {}: {}", p, e),
                };
            }
            "write_file" => {
                let p = as_str_val(args.first().unwrap_or(&V::Null));
                let d = as_str_val(args.get(1).unwrap_or(&V::Null));
                fs::write(&p, d).expect("write_file");
                return V::Null;
            }
            "append_file" => {
                use std::io::Write as _;
                let p = as_str_val(args.first().unwrap_or(&V::Null));
                let d = as_str_val(args.get(1).unwrap_or(&V::Null));
                if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&p) {
                    let _ = f.write_all(d.as_bytes());
                }
                return V::Null;
            }
            "args" => {
                return vl(argv.iter().cloned().map(vs).collect());
            }
            "env" => {
                let k = as_str_val(args.first().unwrap_or(&V::Null));
                return vs(env::var(k).unwrap_or_default());
            }
            "declassify" => return args.first().cloned().unwrap_or(V::Null),
            "exit" => {
                self.exit_code = args.first().map(|x| x.as_i64() as i32).unwrap_or(0);
                self.should_exit = true;
                return V::Null;
            }
            "break" => return vs("__break__".into()),
            "continue" => return vs("__continue__".into()),
            _ => {}
        }
        // H3: O(1) Rc lookup instead of a linear scan + deep clone of the whole fn.
        let f: Rc<V> = match self.fns.get(name) {
            Some(f) => Rc::clone(f),
            None => panic!("unknown function {}", name),
        };
        let mut locals: BTreeMap<String, V> = BTreeMap::new();
        if let V::List(params) = map_get(&f, "params") {
            for (i, p) in params.iter().enumerate() {
                let pname = as_str_val(map_get(p, "name"));
                locals.insert(pname, args.get(i).cloned().unwrap_or(V::Null));
            }
        }
        let flow = if let V::List(body) = map_get(&f, "body") {
            self.exec_stmts(body, &mut locals, argv)
        } else {
            Flow::Val(V::Null)
        };
        match flow {
            Flow::Return(v) => v,
            Flow::Val(v) => v,
            Flow::Break | Flow::Continue => V::Null,
        }
    }

    fn exec_stmts(
        &mut self,
        stmts: &[V],
        locals: &mut BTreeMap<String, V>,
        argv: &mut Vec<String>,
    ) -> Flow {
        let mut last = V::Null;
        for st in stmts {
            if self.should_exit {
                return Flow::Return(V::Null);
            }
            match self.exec_stmt(st, locals, argv) {
                Flow::Return(v) => return Flow::Return(v),
                Flow::Break => return Flow::Break,
                Flow::Continue => return Flow::Continue,
                Flow::Val(v) => last = v,
            }
        }
        Flow::Val(last)
    }

    // Evaluate a braced block (statement list) as a value: the value of its final
    // expression statement. Used by if-expressions.
    fn eval_block(
        &mut self,
        stmts: &[V],
        locals: &mut BTreeMap<String, V>,
        argv: &mut Vec<String>,
    ) -> V {
        match self.exec_stmts(stmts, locals, argv) {
            Flow::Val(v) => v,
            Flow::Return(v) => v,
            _ => V::Null,
        }
    }

    fn exec_stmt(
        &mut self,
        st: &V,
        locals: &mut BTreeMap<String, V>,
        argv: &mut Vec<String>,
    ) -> Flow {
        let k = as_str_val(map_get(st, "kind"));
        match k.as_str() {
            "Let" => {
                let name = as_str_val(map_get(st, "name"));
                let init = self.eval(map_get(st, "init"), locals, argv);
                locals.insert(name, init);
                Flow::Val(V::Null)
            }
            "Assign" => {
                let t = map_get(st, "target");
                // H5b: in-place string append for `x = x + rhs` when x is a string.
                // Avoids O(n^2) accumulation when building large JSON / token strings.
                if as_str_val(map_get(t, "kind")) == "Var" {
                    let tname = as_str_val(map_get(t, "name"));
                    let ve = map_get(st, "value");
                    if as_str_val(map_get(ve, "kind")) == "Binary"
                        && as_str_val(map_get(ve, "op")) == "+"
                    {
                        let lhs = map_get(ve, "lhs");
                        if as_str_val(map_get(lhs, "kind")) == "Var"
                            && as_str_val(map_get(lhs, "name")) == tname
                            && matches!(locals.get(&tname), Some(V::Str(_)))
                        {
                            let rhs = self.eval(map_get(ve, "rhs"), locals, argv);
                            let piece = rhs.display();
                            if let Some(V::Str(s)) = locals.get_mut(&tname) {
                                Rc::make_mut(s).push_str(&piece);
                            }
                            return Flow::Val(V::Null);
                        }
                    }
                }
                let val = self.eval(map_get(st, "value"), locals, argv);
                self.assign(t, val, locals, argv);
                Flow::Val(V::Null)
            }
            "Return" => {
                let v = map_get(st, "value");
                if as_str_val(map_get(v, "kind")) == "None" {
                    return Flow::Return(V::Null);
                }
                Flow::Return(self.eval(v, locals, argv))
            }
            "ExprStmt" => {
                let v = self.eval(map_get(st, "expr"), locals, argv);
                if let V::Str(s) = &v {
                    if s.as_str() == "__break__" {
                        return Flow::Break;
                    }
                    if s.as_str() == "__continue__" {
                        return Flow::Continue;
                    }
                }
                Flow::Val(v)
            }
            "If" => {
                let cond = self.eval(map_get(st, "cond"), locals, argv);
                if cond.truthy() {
                    if let V::List(body) = map_get(st, "then") {
                        let body = Rc::clone(body);
                        return self.exec_stmts(&body, locals, argv);
                    }
                } else {
                    let else_node = map_get(st, "else");
                    if let V::List(body) = else_node {
                        let body = Rc::clone(body);
                        return self.exec_stmts(&body, locals, argv);
                    }
                    if as_bool(map_get(st, "has_else")) {
                        if let V::List(body) = map_get(st, "else") {
                            let body = Rc::clone(body);
                            return self.exec_stmts(&body, locals, argv);
                        }
                    }
                }
                Flow::Val(V::Null)
            }
            "While" => {
                let mut guard = 0u64;
                loop {
                    guard += 1;
                    if guard > 50_000_000 {
                        panic!("while loop exceeded 5e7 iterations (possible non-termination)");
                    }
                    if self.should_exit {
                        break;
                    }
                    let cond = self.eval(map_get(st, "cond"), locals, argv);
                    if !cond.truthy() {
                        break;
                    }
                    if let V::List(body) = map_get(st, "body") {
                        let body = Rc::clone(body);
                        match self.exec_stmts(&body, locals, argv) {
                            Flow::Return(v) => return Flow::Return(v),
                            Flow::Break => break,
                            Flow::Continue => continue,
                            Flow::Val(_) => {}
                        }
                    }
                }
                Flow::Val(V::Null)
            }
            "For" => {
                let iter = map_get(st, "iter");
                let var = as_str_val(map_get(st, "var"));
                if as_str_val(map_get(iter, "kind")) == "Range" {
                    let mut i = self.eval(map_get(iter, "start"), locals, argv).as_i64();
                    let end = self.eval(map_get(iter, "end"), locals, argv).as_i64();
                    let body = match map_get(st, "body") {
                        V::List(b) => Rc::clone(b),
                        _ => Rc::new(vec![]),
                    };
                    while i < end {
                        locals.insert(var.clone(), V::Int(i));
                        match self.exec_stmts(&body, locals, argv) {
                            Flow::Return(v) => return Flow::Return(v),
                            Flow::Break => break,
                            Flow::Continue => {
                                i += 1;
                                continue;
                            }
                            Flow::Val(_) => {}
                        }
                        i += 1;
                    }
                    return Flow::Val(V::Null);
                }
                // for x in <collection> : list elements, map keys (sorted), or string chars.
                let coll = self.eval(iter, locals, argv);
                let seq: Vec<V> = match coll {
                    V::List(xs) => (*xs).clone(),
                    V::Map(m) => m.keys().map(|k| vs(k.clone())).collect(),
                    V::Str(s) => s.chars().map(|c| vs(c.to_string())).collect(),
                    _ => vec![],
                };
                let body = match map_get(st, "body") {
                    V::List(b) => Rc::clone(b),
                    _ => Rc::new(vec![]),
                };
                for item in seq {
                    locals.insert(var.clone(), item);
                    match self.exec_stmts(&body, locals, argv) {
                        Flow::Return(v) => return Flow::Return(v),
                        Flow::Break => break,
                        Flow::Continue => continue,
                        Flow::Val(_) => {}
                    }
                }
                Flow::Val(V::Null)
            }
            _ => Flow::Val(V::Null),
        }
    }

    fn assign(
        &mut self,
        target: &V,
        val: V,
        locals: &mut BTreeMap<String, V>,
        argv: &mut Vec<String>,
    ) {
        let k = as_str_val(map_get(target, "kind"));
        if k == "Var" {
            let n = as_str_val(map_get(target, "name"));
            locals.insert(n, val);
            return;
        }
        if k == "Index" {
            let base_e = map_get(target, "base");
            let idx_e = map_get(target, "index");
            if as_str_val(map_get(base_e, "kind")) == "Var" {
                let n = as_str_val(map_get(base_e, "name"));
                let idx = self.eval(idx_e, locals, argv);
                if let Some(slot) = locals.get_mut(&n) {
                    match slot {
                        V::Map(m) => {
                            Rc::make_mut(m).insert(as_str_val(&idx), val);
                        }
                        V::List(xs) => {
                            let i = idx.as_i64() as usize;
                            let v = Rc::make_mut(xs);
                            if i < v.len() {
                                v[i] = val;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    fn eval(&mut self, e: &V, locals: &mut BTreeMap<String, V>, argv: &mut Vec<String>) -> V {
        if self.should_exit {
            return V::Null;
        }
        let k = as_str_val(map_get(e, "kind"));
        match k.as_str() {
            "None" | "" if matches!(e, V::Null) => V::Null,
            "None" => V::Null,
            "Int" => {
                let s = as_str_val(map_get(e, "value"));
                V::Int(s.parse().unwrap_or(0))
            }
            "Bool" => match map_get(e, "value") {
                V::Bool(b) => V::Bool(*b),
                V::Str(s) => V::Bool(s.as_str() == "true"),
                other => V::Bool(other.truthy()),
            },
            "Str" => map_get(e, "value").clone(),
            "Var" => {
                let n = as_str_val(map_get(e, "name"));
                locals.get(&n).cloned().unwrap_or(V::Null)
            }
            "Call" => {
                let callee = as_str_val(map_get(e, "callee"));
                // push special: O(1) in-place append for `push(local, x)` (all SH push
                // calls are statements; return value unused).
                if callee == "push" {
                    if let V::List(raw_args) = map_get(e, "args") {
                        if let Some(first) = raw_args.first() {
                            if as_str_val(map_get(first, "kind")) == "Var" {
                                let n = as_str_val(map_get(first, "name"));
                                let val = if raw_args.len() > 1 {
                                    let a = raw_args[1].clone();
                                    self.eval(&a, locals, argv)
                                } else {
                                    V::Null
                                };
                                match locals.get_mut(&n) {
                                    Some(V::List(xs)) => Rc::make_mut(xs).push(val),
                                    _ => {
                                        locals.insert(n, vl(vec![val]));
                                    }
                                }
                                return V::Null;
                            }
                        }
                    }
                }
                let mut args = Vec::new();
                if let V::List(xs) = map_get(e, "args") {
                    let xs = Rc::clone(xs);
                    for a in xs.iter() {
                        args.push(self.eval(a, locals, argv));
                    }
                }
                self.call(&callee, args, argv)
            }
            "Binary" => {
                let op = as_str_val(map_get(e, "op"));
                let l = self.eval(map_get(e, "lhs"), locals, argv);
                let r = self.eval(map_get(e, "rhs"), locals, argv);
                self.eval_bin(&op, &l, &r)
            }
            "Unary" => {
                let op = as_str_val(map_get(e, "op"));
                let x = self.eval(map_get(e, "expr"), locals, argv);
                if op == "!" {
                    V::Bool(!x.truthy())
                } else if op == "-" {
                    V::Int(-x.as_i64())
                } else {
                    x
                }
            }
            "Index" => {
                let base = self.eval(map_get(e, "base"), locals, argv);
                let idx = self.eval(map_get(e, "index"), locals, argv);
                match base {
                    V::List(xs) => xs.get(idx.as_i64() as usize).cloned().unwrap_or(V::Null),
                    V::Map(m) => m.get(&as_str_val(&idx)).cloned().unwrap_or(V::Null),
                    V::Str(s) => {
                        let i = idx.as_i64();
                        if i >= 0 {
                            vs(s
                                .as_bytes()
                                .get(i as usize)
                                .map(|b| (*b as char).to_string())
                                .unwrap_or_default())
                        } else {
                            vs(String::new())
                        }
                    }
                    _ => V::Null,
                }
            }
            "List" => {
                let mut xs = Vec::new();
                if let V::List(els) = map_get(e, "elements") {
                    let els = Rc::clone(els);
                    for el in els.iter() {
                        xs.push(self.eval(el, locals, argv));
                    }
                }
                vl(xs)
            }
            "Map" => {
                let mut m = BTreeMap::new();
                let keys = match map_get(e, "keys") {
                    V::List(xs) => Rc::clone(xs),
                    _ => Rc::new(vec![]),
                };
                let vals = match map_get(e, "vals") {
                    V::List(xs) => Rc::clone(xs),
                    _ => Rc::new(vec![]),
                };
                for i in 0..keys.len() {
                    let key = match &keys[i] {
                        V::Str(s) => (**s).clone(),
                        other if as_str_val(map_get(other, "kind")) == "Str" => {
                            as_str_val(map_get(other, "value"))
                        }
                        other => as_str_val(other),
                    };
                    let v = if i < vals.len() {
                        self.eval(&vals[i], locals, argv)
                    } else {
                        V::Null
                    };
                    m.insert(key, v);
                }
                vm(m)
            }
            "EnumInit" => {
                let ty = as_str_val(map_get(e, "ty"));
                let variant = as_str_val(map_get(e, "variant"));
                let shape = as_str_val(map_get(e, "shape"));
                if shape == "tuple" {
                    let mut tuple = Vec::new();
                    if let V::List(xs) = map_get(e, "tuple") {
                        let xs = Rc::clone(xs);
                        for a in xs.iter() {
                            tuple.push(self.eval(a, locals, argv));
                        }
                    }
                    V::Enum(ty, variant, tuple, vec![])
                } else if shape == "struct" {
                    let fnames = match map_get(e, "fnames") {
                        V::List(xs) => Rc::clone(xs),
                        _ => Rc::new(vec![]),
                    };
                    let fexprs = match map_get(e, "fexprs") {
                        V::List(xs) => Rc::clone(xs),
                        _ => Rc::new(vec![]),
                    };
                    let mut fields = Vec::new();
                    for i in 0..fnames.len() {
                        let fname = as_str_val(&fnames[i]);
                        let fv = if i < fexprs.len() {
                            self.eval(&fexprs[i], locals, argv)
                        } else {
                            V::Null
                        };
                        fields.push((fname, fv));
                    }
                    V::Enum(ty, variant, vec![], fields)
                } else {
                    V::Enum(ty, variant, vec![], vec![])
                }
            }
            "IfExpr" => {
                let cond = self.eval(map_get(e, "cond"), locals, argv);
                if cond.truthy() {
                    if let V::List(body) = map_get(e, "then") {
                        let body = Rc::clone(body);
                        return self.eval_block(&body, locals, argv);
                    }
                } else if let V::List(body) = map_get(e, "else") {
                    let body = Rc::clone(body);
                    return self.eval_block(&body, locals, argv);
                }
                V::Null
            }
            "Match" => {
                let scrut = self.eval(map_get(e, "scrut"), locals, argv);
                let arms = match map_get(e, "arms") {
                    V::List(xs) => Rc::clone(xs),
                    _ => Rc::new(vec![]),
                };
                for arm in arms.iter() {
                    let pat = map_get(arm, "pat");
                    if let Some(binds) = self.match_pat(pat, &scrut) {
                        for (bn, bv) in binds {
                            locals.insert(bn, bv);
                        }
                        let body = map_get(arm, "body").clone();
                        return self.eval(&body, locals, argv);
                    }
                }
                // H1: fail-closed. A non-exhaustive match must not silently yield Null
                // (the host panics ANUBIS_MATCH_UNMATCHED — align with it).
                panic!("ANUBIS_SH_MATCH_UNMATCHED");
            }
            "Range" => e.clone(),
            "UnsupportedExpr" => V::Null,
            _ => V::Null,
        }
    }

    // Try to match a pattern against a value. Returns the bindings on success.
    fn match_pat(&self, pat: &V, val: &V) -> Option<Vec<(String, V)>> {
        let pk = as_str_val(map_get(pat, "pk"));
        match pk.as_str() {
            "wild" => Some(vec![]),
            "enum" => {
                let want_variant = as_str_val(map_get(pat, "variant"));
                if let V::Enum(_ty, variant, tuple, fields) = val {
                    if variant != &want_variant {
                        return None;
                    }
                    let shape = as_str_val(map_get(pat, "shape"));
                    let mut binds = Vec::new();
                    if shape == "tuple" {
                        if let V::List(names) = map_get(pat, "binds") {
                            for (i, nm) in names.iter().enumerate() {
                                let name = as_str_val(nm);
                                if name != "_" {
                                    binds.push((name, tuple.get(i).cloned().unwrap_or(V::Null)));
                                }
                            }
                        }
                    } else if shape == "struct" {
                        let fnames = match map_get(pat, "fnames") {
                            V::List(xs) => Rc::clone(xs),
                            _ => Rc::new(vec![]),
                        };
                        let bnames = match map_get(pat, "binds") {
                            V::List(xs) => Rc::clone(xs),
                            _ => Rc::new(vec![]),
                        };
                        for i in 0..fnames.len() {
                            let fname = as_str_val(&fnames[i]);
                            let bname = if i < bnames.len() {
                                as_str_val(&bnames[i])
                            } else {
                                fname.clone()
                            };
                            if bname != "_" {
                                let fv = fields
                                    .iter()
                                    .find(|(k, _)| k == &fname)
                                    .map(|(_, v)| v.clone())
                                    .unwrap_or(V::Null);
                                binds.push((bname, fv));
                            }
                        }
                    }
                    Some(binds)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn eval_bin(&self, op: &str, l: &V, r: &V) -> V {
        match op {
            "+" => {
                if matches!(l, V::Str(_)) || matches!(r, V::Str(_)) {
                    vs(format!("{}{}", l.display(), r.display()))
                } else {
                    V::Int(l.as_i64() + r.as_i64())
                }
            }
            "-" => V::Int(l.as_i64() - r.as_i64()),
            "*" => V::Int(l.as_i64() * r.as_i64()),
            "/" => {
                let d = r.as_i64();
                V::Int(if d == 0 { 0 } else { l.as_i64() / d })
            }
            "%" => {
                let d = r.as_i64();
                V::Int(if d == 0 { 0 } else { l.as_i64() % d })
            }
            "==" => V::Bool(match (l, r) {
                (V::Int(a), V::Int(b)) => a == b,
                (V::Bool(a), V::Bool(b)) => a == b,
                (V::Str(a), V::Str(b)) => a == b,
                _ => l.display() == r.display(),
            }),
            "!=" => V::Bool(match (l, r) {
                (V::Int(a), V::Int(b)) => a != b,
                (V::Bool(a), V::Bool(b)) => a != b,
                (V::Str(a), V::Str(b)) => a != b,
                _ => l.display() != r.display(),
            }),
            "<" => V::Bool(l.as_i64() < r.as_i64()),
            "<=" => V::Bool(l.as_i64() <= r.as_i64()),
            ">" => V::Bool(l.as_i64() > r.as_i64()),
            ">=" => V::Bool(l.as_i64() >= r.as_i64()),
            "&&" => V::Bool(l.truthy() && r.truthy()),
            "||" => V::Bool(l.truthy() || r.truthy()),
            _ => V::Null,
        }
    }
}

enum Flow {
    Val(V),
    Return(V),
    Break,
    Continue,
}

/// Run SH program JSON; returns process exit code.
pub fn sh_run(payload: &str, argv: Vec<String>) -> i32 {
    // H2: run on a large stack — a self-hosting compiler's recursive descent plus
    // enum-value recursion can exceed the default ~8 MB main-thread stack.
    let payload = payload.to_string();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || sh_run_inner(&payload, argv))
        .expect("spawn interp thread")
        .join()
        .expect("interp thread panicked")
}

fn sh_run_inner(payload: &str, mut argv: Vec<String>) -> i32 {
    let program = parse_json(payload);
    // H3: build the Rc function table once (name -> Rc<fn-item>).
    let mut fns: BTreeMap<String, Rc<V>> = BTreeMap::new();
    if let V::List(items) = map_get(&program, "items") {
        for it in items.iter() {
            if as_str_val(map_get(it, "kind")) == "Fn" {
                fns.insert(as_str_val(map_get(it, "name")), Rc::new(it.clone()));
            }
        }
    }
    let mut rt = Rt {
        program,
        fns,
        exit_code: 0,
        should_exit: false,
    };
    let _ = rt.call("main", vec![], &mut argv);
    rt.exit_code
}

// When used as the package body, main is supplied by codegen after PAYLOAD.
