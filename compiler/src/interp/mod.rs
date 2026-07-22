//! Fast AST interpreter for `anubis repl` (default mode).
//! Not a full substitute for `anubis run` — use `--exact` for production fidelity.

use crate::frontend::{Expr, Item, Stmt};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    List(Vec<Value>),
    Unit,
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{n}"),
            Value::Float(x) => write!(f, "{x}"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Str(s) => write!(f, "{s}"),
            Value::List(xs) => {
                write!(f, "[")?;
                for (i, x) in xs.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{x}")?;
                }
                write!(f, "]")
            }
            Value::Unit => write!(f, "()"),
        }
    }
}

#[derive(Debug, Default)]
pub struct Interp {
    pub env: BTreeMap<String, Value>,
    pub fns: BTreeMap<String, Item>,
    pub output: String,
}

impl Interp {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load_items(&mut self, items: &[Item]) {
        for it in items {
            if let Item::Fn { name, .. } = it {
                self.fns.insert(name.clone(), it.clone());
            }
        }
    }

    pub fn eval_program(&mut self, items: &[Item]) -> Result<Value, String> {
        self.load_items(items);
        if let Some(Item::Fn { body, .. }) = self.fns.get("main").cloned() {
            return self.eval_stmts(&body);
        }
        // No main: execute top-level nothing
        Ok(Value::Unit)
    }

    pub fn eval_stmts(&mut self, stmts: &[Stmt]) -> Result<Value, String> {
        let mut last = Value::Unit;
        for st in stmts {
            last = self.eval_stmt(st)?;
        }
        Ok(last)
    }

    fn eval_stmt(&mut self, st: &Stmt) -> Result<Value, String> {
        match st {
            Stmt::Let { name, init, .. } => {
                let v = self.eval_expr(init)?;
                self.env.insert(name.clone(), v);
                Ok(Value::Unit)
            }
            Stmt::Assign { target, value } => {
                let v = self.eval_expr(value)?;
                if let Expr::Var(name) = target {
                    self.env.insert(name.clone(), v);
                    Ok(Value::Unit)
                } else {
                    Err("ANUBIS_REPL_UNSUPPORTED: complex assignment (use --exact)".into())
                }
            }
            Stmt::ExprStmt(e) => {
                // `return expr` is Call { callee: "return", ... }
                if let Expr::Call { callee, args } = e {
                    if callee == "return" {
                        if args.is_empty() {
                            return Ok(Value::Unit);
                        }
                        return self.eval_expr(&args[0]);
                    }
                }
                self.eval_expr(e)
            }
            Stmt::If { cond, then, else_ } => {
                let c = self.eval_expr(cond)?;
                if truthy(&c) {
                    self.eval_stmts(then)
                } else if let Some(el) = else_ {
                    self.eval_stmts(el)
                } else {
                    Ok(Value::Unit)
                }
            }
            Stmt::While { cond, body, .. } => {
                while truthy(&self.eval_expr(cond)?) {
                    self.eval_stmts(body)?;
                }
                Ok(Value::Unit)
            }
            _ => Err(
                "ANUBIS_REPL_UNSUPPORTED: statement form not in fast interpreter (use --exact)"
                    .into(),
            ),
        }
    }

    pub fn eval_expr(&mut self, e: &Expr) -> Result<Value, String> {
        match e {
            Expr::Var(n) => self
                .env
                .get(n)
                .cloned()
                .or_else(|| {
                    if self.fns.contains_key(n) {
                        Some(Value::Str(format!("<fn {n}>")))
                    } else {
                        None
                    }
                })
                .ok_or_else(|| format!("ANUBIS_REPL_UNSUPPORTED: unbound `{n}`")),
            Expr::Literal(s) => parse_literal(s),
            Expr::StrLiteral(s) => Ok(Value::Str(s.clone())),
            Expr::Binary { op, lhs, rhs } => {
                let l = self.eval_expr(lhs)?;
                let r = self.eval_expr(rhs)?;
                eval_bin(op, &l, &r)
            }
            Expr::Unary { op, expr } => {
                let v = self.eval_expr(expr)?;
                match (op.as_str(), v) {
                    ("-", Value::Int(n)) => Ok(Value::Int(-n)),
                    ("!", Value::Bool(b)) => Ok(Value::Bool(!b)),
                    _ => Err("ANUBIS_REPL_UNSUPPORTED: unary".into()),
                }
            }
            Expr::Call { callee, args } => {
                if callee == "print" || callee == "println" {
                    let mut parts = Vec::new();
                    for a in args {
                        parts.push(self.eval_expr(a)?.to_string());
                    }
                    let line = parts.join(" ");
                    self.output.push_str(&line);
                    self.output.push('\n');
                    return Ok(Value::Unit);
                }
                if callee == "len" && args.len() == 1 {
                    return match self.eval_expr(&args[0])? {
                        Value::List(xs) => Ok(Value::Int(xs.len() as i64)),
                        Value::Str(s) => Ok(Value::Int(s.len() as i64)),
                        _ => Err("ANUBIS_REPL_UNSUPPORTED: len".into()),
                    };
                }
                let f = self.fns.get(callee).cloned().ok_or_else(|| {
                    format!("ANUBIS_REPL_UNSUPPORTED: unknown function `{callee}` (use --exact)")
                })?;
                if let Item::Fn { params, body, .. } = f {
                    if params.len() != args.len() {
                        return Err(format!("ANUBIS_REPL_UNSUPPORTED: arity {callee}"));
                    }
                    let mut vals = Vec::new();
                    for a in args {
                        vals.push(self.eval_expr(a)?);
                    }
                    let saved = self.env.clone();
                    for ((n, _), v) in params.iter().zip(vals) {
                        self.env.insert(n.clone(), v);
                    }
                    let r = self.eval_stmts(&body);
                    self.env = saved;
                    return r;
                }
                Err("ANUBIS_REPL_UNSUPPORTED: call".into())
            }
            Expr::ArrayLiteral { elements } => {
                let mut xs = Vec::new();
                for el in elements {
                    xs.push(self.eval_expr(el)?);
                }
                Ok(Value::List(xs))
            }
            _ => Err(
                "ANUBIS_REPL_UNSUPPORTED: expression form not in fast interpreter (use --exact)"
                    .into(),
            ),
        }
    }
}

fn parse_literal(s: &str) -> Result<Value, String> {
    if s == "true" {
        return Ok(Value::Bool(true));
    }
    if s == "false" {
        return Ok(Value::Bool(false));
    }
    if let Ok(n) = s.parse::<i64>() {
        return Ok(Value::Int(n));
    }
    if let Ok(x) = s.parse::<f64>() {
        return Ok(Value::Float(x));
    }
    Ok(Value::Str(s.to_string()))
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Int(n) => *n != 0,
        Value::Unit => false,
        _ => true,
    }
}

fn eval_bin(op: &str, l: &Value, r: &Value) -> Result<Value, String> {
    match (op, l, r) {
        ("+", Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
        ("-", Value::Int(a), Value::Int(b)) => Ok(Value::Int(a - b)),
        ("*", Value::Int(a), Value::Int(b)) => Ok(Value::Int(a * b)),
        ("/", Value::Int(a), Value::Int(b)) => {
            if *b == 0 {
                Err("ANUBIS_REPL_UNSUPPORTED: division by zero".into())
            } else {
                Ok(Value::Int(a / b))
            }
        }
        ("%", Value::Int(a), Value::Int(b)) => Ok(Value::Int(a % b)),
        ("+", Value::Str(a), Value::Str(b)) => Ok(Value::Str(format!("{a}{b}"))),
        ("==", a, b) => Ok(Value::Bool(values_eq(a, b))),
        ("!=", a, b) => Ok(Value::Bool(!values_eq(a, b))),
        ("<", Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a < b)),
        (">", Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a > b)),
        ("<=", Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a <= b)),
        (">=", Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a >= b)),
        ("&&", Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(*a && *b)),
        ("||", Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(*a || *b)),
        _ => Err(format!(
            "ANUBIS_REPL_UNSUPPORTED: binary `{op}` on these values"
        )),
    }
}

fn values_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => x == y,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::parse_source;

    #[test]
    fn interp_print_add() {
        let src = "fn main() { print(2 + 3); }";
        let ast = parse_source(src).unwrap();
        let mut i = Interp::new();
        i.eval_program(&ast.items).unwrap();
        assert!(i.output.contains('5'), "{}", i.output);
    }
}
