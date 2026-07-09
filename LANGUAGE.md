# The Anubis Language Reference

Anubis is a small, dynamically-typed, Turing-complete systems/scripting language with a
security-research surface (taint tracking, symbolic obligations, and zero-knowledge proof
lowering). This document describes the **general-purpose language** — the part you use with
`anubis run`. The research/proof surface is summarized at the end and in `SOUL.md`.

## Execution model

`anubis run FILE` compiles the whole program — every function, not just `main` — to a
self-contained Rust program, invokes `rustc`, and executes the resulting binary. User-defined
calls and recursion run on the native call stack; `while`/`loop`/`for` become native loops;
bindings are mutable. Together with conditionals and unbounded heap growth this makes the
language Turing-complete.

The same lowering targets a RISC0 zkVM guest for `anubis prove`, so the program you run is the
program that gets proven.

Every runtime value is one of nine kinds: **int** (`i64`), **float** (`f64`), **bool**,
**string**, **list**, **map**, **struct**, **enum**, and **closure**.

```
fn main() {
    print("hello, anubis");
}
```

Statements are newline-terminated; a trailing `;` is **optional** on every statement kind
(`let`, assignment, `return`, and expression statements alike). The examples in this document use
`;` for clarity, but omitting it is equally valid.

## Comments

```
// line comment
/* block comment /* may nest */ still a comment */
```

## Literals

| Kind    | Examples |
|---------|----------|
| Integer | `42`, `1_000_000`, `0xFF`, `0b1010`, `0o17` |
| Float   | `3.14`, `1.0`, `1e9`, `1.5e-3` |
| Bool    | `true`, `false` |
| String  | `"hi"`, `"tab\tnewline\n"`, `"\x41"`, `"\u{1F600}"` |
| Char    | `'a'`, `'\n'` — a one-character string |
| List    | `[1, 2, 3]`, `[]`, `["a", true, 3.5]` (heterogeneous) |
| Map     | `{ "a": 1, "b": 2 }` (string keys) |

Underscores in numbers are separators. `\n \t \r \0 \\ \" \'`, `\xNN`, and `\u{...}` escapes are
supported in strings and char literals. Numeric-looking strings stay strings: `"007"` prints
`007`, and `len("3")` is `1`.

## Variables and mutation

All bindings are reassignable (`mut` is accepted but never required):

```
let x = 1;
x = x + 1;          // reassign
let mut y = 10;     // `mut` is optional
```

Assignable *places* can be arbitrarily nested — variables, indices, fields, and any chain of
them:

```
a[i] = v;
p.field = v;
grid["row"][col].value = v;
```

Compound assignment works on any place: `+= -= *= /= %= &= |= ^= <<= >>=`.

## Operators

From highest precedence to lowest:

| Tier | Operators |
|------|-----------|
| unary | `-x`  `!x`  `~x` |
| multiplicative | `*`  `/`  `%` |
| additive | `+`  `-` |
| shift | `<<`  `>>` |
| comparison | `<`  `<=`  `>`  `>=`  `==`  `!=` |
| bitwise-and | `&` |
| bitwise-xor | `^` |
| bitwise-or | `\|` |
| logical-and | `&&` |
| logical-or | `\|\|` |

Notes:

- `+` is overloaded: numbers add, strings and lists concatenate (`[1,2] + [3]` → `[1,2,3]`,
  `"a" + 1` → `"a1"`).
- Integer `+ - * /` wrap on overflow; **integer division/modulo by zero panics** (fail-closed).
- If either operand is a float, arithmetic promotes to float; float division follows IEEE
  (`1.0 / 0.0` is `inf`).
- Comparisons between two integers are exact `i64` comparisons (no float rounding, even above
  2^53); mixed int/float comparisons use `f64`; strings and other values compare by their display
  form (lexicographic for `< <= > >=`). So `1 == 1.0` is `true` and `"apple" < "banana"` is `true`.
- `&&` and `\|\|` short-circuit.
- Bitwise/shift operate on the integer view of their operands.
- `expr as T` converts: casts to an integer type truncate toward zero and wrap to that width
  (`3.9 as u32` is `3`, `300 as u8` is `44`, `-1 as u8` is `255`); casts to a float type convert
  to `f64`; pointer casts (research surface) leave the value unchanged.

## Control flow

```
// if / else if / else — statement form
if x > 0 { print("pos"); } else if x < 0 { print("neg"); } else { print("zero"); }

// if as an expression (else required); branches may contain statements then a trailing value
let label = if n % 2 == 0 { let k = n / 2; str(k) } else { "odd" };

// while, loop, break, continue
let i = 0;
while i < 10 { if i == 5 { break; } i = i + 1; }

// for over a numeric range [a, b)
for i in 0..n { print(i); }

// for over a collection (list, string chars, or map keys)
for x in [10, 20, 30] { print(x); }
for k in { "a": 1 } { print(k); }
```

## Functions and recursion

Parameter types are optional (the language is dynamically typed); a `-> Type` return annotation
is accepted and recorded but does not constrain the runtime.

```
fn add(a, b) { return a + b; }
fn fib(n: u32) -> u32 {
    if n < 2 { return n; }
    return fib(n - 1) + fib(n - 2);
}
```

**Implicit return.** A function's final bare expression is its value — an explicit `return` is
optional, exactly as in Rust or ML. A trailing statement (a loop, an assignment, a `print`) or an
empty body yields the default value `0`.

```
fn double(n) { n * 2 }                 // returns n*2, no `return` needed
fn label(n)  { if n > 0 { "pos" } else { "neg" } }   // tail if-expression
fn area(sh)  { match sh { Circle(r) => 3*r*r, _ => 0 } }  // tail match
```

An explicit `return` anywhere (including inside a `match` arm) returns from the whole function.

Duplicate function definitions, duplicate parameters, arity mismatches on calls to known
functions, and calls to unknown functions are compile-time errors.

## Closures and higher-order functions

Lambdas are first-class values that capture their environment by value:

```
let inc = |x| x + 1;
print(inc(41));                       // 42

fn make_adder(n) { return |x| x + n; }
let add10 = make_adder(10);
print(add10(5));                      // 15

// block-bodied lambda
let f = |x| { let y = x * x; y + 1 };
```

Higher-order builtins take closures: `map`, `filter`, `reduce`, `each`, `find`, `any`, `all`,
`count`, `sort_by`, `apply`, and `call`.

```
print(map([1, 2, 3], |x| x * x));                 // [1, 4, 9]
print(filter(range(1, 11), |x| x % 2 == 0));      // [2, 4, 6, 8, 10]
print(reduce([1, 2, 3, 4], |a, b| a + b, 0));     // 10
print(sort_by(people, |p| p.age));
```

A user-defined function always takes precedence over a builtin of the same name, so builtin
names are effectively reservable.

## Structs

```
struct Point { x: u32, y: u32 }

fn main() {
    let p = Point { x: 3, y: 4 };
    print(p.x + p.y);   // 7
    p.x = 30;           // fields are mutable places
    print(p);           // Point { x: 30, y: 4 }
}
```

## Enums and pattern matching

Enums support unit, tuple, and struct-shaped variants. `match` is an expression (it yields a
value) and can also stand as a statement. Arms are tried top-to-bottom; the first arm whose
pattern matches — and whose guard, if any, passes — wins.

```
enum Http {
    Ok { code: u32 },
    Redirect(u32),
    NotFound,
}

fn describe(r) {
    match r {
        Http::Ok { code: c } => c,       // struct variant, binds field `code` to `c`
        Http::Redirect(loc)  => loc,     // tuple variant, binds payload to `loc`
        Http::NotFound       => 404,     // unit variant
        _                    => 0,        // wildcard
    }
}
```

### Pattern kinds

| Pattern | Example | Matches |
|---------|---------|---------|
| literal | `0`, `-1`, `3.14`, `true`, `"hi"` | a value of the **same kind** and equal (see below) |
| binding | `n` | anything; binds the scrutinee to `n` (irrefutable) |
| wildcard | `_` | anything; binds nothing |
| or-pattern | `1 \| 2 \| 3` | any listed alternative (alternatives may not bind) |
| enum tuple | `Status::Err(n)`, `Pair(_, b)` | that variant; binds fields positionally (`_` ignores one) |
| enum struct | `Http::Ok { code: c }` | that variant; binds named fields |

Literal patterns are **type-exact**: a string pattern matches only strings, a bool pattern only
bools, and a numeric pattern only numbers. Unlike the `==` operator (which coerces across types),
`match 5 { "5" => … }` does **not** match, and `match 1 { true => … }` does **not** match. Int and
float remain interchangeable when numerically equal (`match 5 { 5.0 => … }` matches).

### Guards

Any arm may carry an `if <condition>` guard evaluated after the pattern binds. If the guard is
false, matching **falls through** to the next arm:

```
fn grade(n) {
    match n {
        n if n >= 90 => "A",
        n if n >= 80 => "B",
        n if n >= 70 => "C",
        _            => "F",
    }
}
```

### Or-patterns and literals

```
fn kind(n) {
    match n {
        0            => "zero",
        1 | 2 | 3    => "small",
        n if n < 0   => "negative",
        _            => "other",
    }
}
```

Exhaustiveness: a `match` on a known enum type must cover every variant or include an
irrefutable arm (`_` or a bare binding). Guarded arms do not count toward coverage, since a
guard may fail. Nested matches compose freely — a match may appear in another match's arm, in a
loop body, as a function argument, or inside a closure.

## Standard library

**Conversions / reflection:** `str`, `int`, `float`, `bool`, `type`, `parse_int`, `parse_float`,
`len`.

**Math:** `abs`, `min`, `max` (variadic or over a list), `pow`, `sqrt`, `floor`, `ceil`,
`round`, `gcd`.

**Strings:** `upper`, `lower`, `trim`, `split`, `join`, `contains`, `starts_with`, `ends_with`,
`replace`, `index_of`, `substr`, `char_at`, `ord`, `chr`, `repeat`, `reverse`.

**Lists:** `push`, `pop`, `insert`, `remove`, `slice`, `reverse`, `sort`, `sort_by`, `sum`,
`range` (2- or 3-arg), `contains`, `index_of`, `map`, `filter`, `reduce`, `each`, `find`, `any`,
`all`, `count`. Indexing accepts negatives (`xs[-1]` is the last element).

**Maps:** `keys`, `values`, `has_key`, `remove`, `len`. `for k in m` iterates keys.

**I/O and control:** `print`, `println`, `eprint`, `eprintln` (space-separated args; zero args =
blank line), `input` / `read_line` (a line from stdin), `args` (command-line arguments),
`assert(cond)` (panics fail-closed on false), and `panic(msg)`.

## Modules and imports

```
import bounty.net;

module util {
    struct Pair { a: u32, b: u32 }
    fn make() { return Pair { a: 1, b: 2 }; }
}
```

Modules group items; the call namespace is flat.

## Research and proof surface (summary)

Beyond the general-purpose language, Anubis has a security-research layer used by `anubis build`,
`anubis check`, and `anubis prove`:

- `research { ... }` / `exploit { ... }` blocks, `tainted<T>` types, `symbolic()`,
  `assume(...)`, `assert(...)`, `taint_source(...)`, `declassify(value, policy: "...", reason:
  "...")`, and `sink(...)`. In safe mode a tainted value reaching a sink without a proper
  declassify is a compile-time error.
- `hybrid { gpu { } cpu { } prove { } }` blocks lower to a Metal + RISC0 proving pipeline.
- `proof_input_*` / `proof_commit_*` / `proof_assert` bind a program to a zero-knowledge receipt
  whose image ID is derived from *this* program.

These constructs are analysis/proof-oriented and are not all available in ordinary `anubis run`;
see `SOUL.md` and `README.md` for the proof pipeline.

## Command-line

```
anubis run FILE [--allow-research] [-- args...]   # compile + execute
anubis build FILE [--evidence|--bounty]           # build with tamper-evident evidence
anubis prove ...                                   # generate a RISC0 receipt bound to the program
anubis check FILE                                  # semantic / taint / solver checks
```

See `examples/tour/` for a runnable tour of every language feature (each program carries an
`// EXPECT:` header verified by the compiler's golden test suite).
