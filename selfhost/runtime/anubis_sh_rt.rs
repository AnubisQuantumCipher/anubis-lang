// Anubis-SH minimal runtime (deterministic). Embedded by SH codegen.
use std::collections::BTreeMap;
use std::io::Write;

#[derive(Clone, Debug)]
pub enum AnubisValue {
    Int(i64),
    Bool(bool),
    Str(String),
    List(Vec<AnubisValue>),
    Map(BTreeMap<String, AnubisValue>),
    Unit,
}

impl AnubisValue {
    pub fn display(&self) -> String {
        match self {
            AnubisValue::Int(n) => n.to_string(),
            AnubisValue::Bool(b) => b.to_string(),
            AnubisValue::Str(s) => s.clone(),
            AnubisValue::List(xs) => {
                let parts: Vec<_> = xs.iter().map(|x| x.display()).collect();
                format!("[{}]", parts.join(", "))
            }
            AnubisValue::Map(_) => "{...}".into(),
            AnubisValue::Unit => "()".into(),
        }
    }
    pub fn truthy(&self) -> bool {
        match self {
            AnubisValue::Bool(b) => *b,
            AnubisValue::Int(n) => *n != 0,
            AnubisValue::Str(s) => !s.is_empty(),
            AnubisValue::List(xs) => !xs.is_empty(),
            AnubisValue::Unit => false,
            AnubisValue::Map(m) => !m.is_empty(),
        }
    }
    pub fn as_i64(&self) -> i64 {
        match self {
            AnubisValue::Int(n) => *n,
            AnubisValue::Bool(b) => {
                if *b {
                    1
                } else {
                    0
                }
            }
            _ => 0,
        }
    }
    pub fn add_val(&self, o: &AnubisValue) -> AnubisValue {
        match (self, o) {
            (AnubisValue::Str(a), b) => AnubisValue::Str(format!("{}{}", a, b.display())),
            (a, AnubisValue::Str(b)) => AnubisValue::Str(format!("{}{}", a.display(), b)),
            _ => AnubisValue::Int(self.as_i64() + o.as_i64()),
        }
    }
    pub fn sub_val(&self, o: &AnubisValue) -> AnubisValue {
        AnubisValue::Int(self.as_i64() - o.as_i64())
    }
    pub fn mul_val(&self, o: &AnubisValue) -> AnubisValue {
        AnubisValue::Int(self.as_i64() * o.as_i64())
    }
    pub fn div_val(&self, o: &AnubisValue) -> AnubisValue {
        let d = o.as_i64();
        AnubisValue::Int(if d == 0 { 0 } else { self.as_i64() / d })
    }
    pub fn mod_val(&self, o: &AnubisValue) -> AnubisValue {
        let d = o.as_i64();
        AnubisValue::Int(if d == 0 { 0 } else { self.as_i64() % d })
    }
    pub fn neg_val(&self) -> AnubisValue {
        AnubisValue::Int(-self.as_i64())
    }
    pub fn eq_val(&self, o: &AnubisValue) -> bool {
        match (self, o) {
            (AnubisValue::Int(a), AnubisValue::Int(b)) => a == b,
            (AnubisValue::Bool(a), AnubisValue::Bool(b)) => a == b,
            (AnubisValue::Str(a), AnubisValue::Str(b)) => a == b,
            _ => self.display() == o.display(),
        }
    }
    pub fn lt_val(&self, o: &AnubisValue) -> bool {
        self.as_i64() < o.as_i64()
    }
    pub fn le_val(&self, o: &AnubisValue) -> bool {
        self.as_i64() <= o.as_i64()
    }
    pub fn gt_val(&self, o: &AnubisValue) -> bool {
        self.as_i64() > o.as_i64()
    }
    pub fn ge_val(&self, o: &AnubisValue) -> bool {
        self.as_i64() >= o.as_i64()
    }
    pub fn len_val(&self) -> AnubisValue {
        match self {
            AnubisValue::Str(s) => AnubisValue::Int(s.chars().count() as i64),
            AnubisValue::List(xs) => AnubisValue::Int(xs.len() as i64),
            AnubisValue::Map(m) => AnubisValue::Int(m.len() as i64),
            _ => AnubisValue::Int(0),
        }
    }
    pub fn char_at(&self, i: &AnubisValue) -> AnubisValue {
        match self {
            AnubisValue::Str(s) => {
                let chars: Vec<char> = s.chars().collect();
                let k = i.as_i64() as usize;
                if k < chars.len() {
                    AnubisValue::Str(chars[k].to_string())
                } else {
                    AnubisValue::Str(String::new())
                }
            }
            _ => AnubisValue::Str(String::new()),
        }
    }
    pub fn ord_val(&self) -> AnubisValue {
        match self {
            AnubisValue::Str(s) => AnubisValue::Int(s.chars().next().map(|c| c as i64).unwrap_or(0)),
            _ => AnubisValue::Int(0),
        }
    }
    pub fn substr_val(&self, start: &AnubisValue, lenv: &AnubisValue) -> AnubisValue {
        match self {
            AnubisValue::Str(s) => {
                let chars: Vec<char> = s.chars().collect();
                let st = start.as_i64().max(0) as usize;
                let ln = lenv.as_i64().max(0) as usize;
                AnubisValue::Str(chars.into_iter().skip(st).take(ln).collect())
            }
            _ => AnubisValue::Str(String::new()),
        }
    }
    pub fn push_val(&mut self, v: AnubisValue) {
        if let AnubisValue::List(xs) = self {
            xs.push(v);
        }
    }
    pub fn index_get(&self, i: &AnubisValue) -> AnubisValue {
        match self {
            AnubisValue::List(xs) => {
                let k = i.as_i64() as usize;
                xs.get(k).cloned().unwrap_or(AnubisValue::Unit)
            }
            AnubisValue::Map(m) => m.get(&i.display()).cloned().unwrap_or(AnubisValue::Unit),
            AnubisValue::Str(_) => self.char_at(i),
            _ => AnubisValue::Unit,
        }
    }
    pub fn index_set(&mut self, i: &AnubisValue, v: AnubisValue) {
        match self {
            AnubisValue::List(xs) => {
                let k = i.as_i64() as usize;
                if k < xs.len() {
                    xs[k] = v;
                }
            }
            AnubisValue::Map(m) => {
                m.insert(i.display(), v);
            }
            _ => {}
        }
    }
    pub fn read_file(path: &AnubisValue) -> AnubisValue {
        let p = path.display();
        match std::fs::read_to_string(&p) {
            Ok(s) => AnubisValue::Str(s),
            Err(e) => panic!("read_file {}: {}", p, e),
        }
    }
    pub fn write_file(path: &AnubisValue, data: &AnubisValue) {
        let p = path.display();
        std::fs::write(&p, data.display()).expect("write_file");
    }
    pub fn args_val() -> AnubisValue {
        let mut a: Vec<AnubisValue> = std::env::args().skip(1).map(AnubisValue::Str).collect();
        AnubisValue::List(a)
    }
}
