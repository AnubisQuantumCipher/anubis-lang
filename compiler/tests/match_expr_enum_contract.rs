//! Contract obligations in value-position matches follow runtime arm reachability.
//! These Safe-only tests inspect obligation generation; the soundness matrix checks
//! the solver verdicts and the paired native behavior separately.
use anubis_compiler::{
    frontend::{parse_source, Mode},
    middle::typecheck_ex,
};

fn has_call_precondition(source: &str) -> bool {
    let ast = parse_source(source).expect("valid Safe fixture");
    let ir = typecheck_ex(ast, Mode::Safe, false).expect("Safe analysis");
    ir.solver_obligations
        .iter()
        .any(|obligation| obligation.name.starts_with("requires@f:"))
}

const PRELUDE: &str = "fn f(x: i64) -> i64 requires(x > 0) { return x; }\n";

#[test]
fn exact_enum_payload_miss_skips_only_the_dead_value_arm() {
    let dead = format!(
        "{PRELUDE} fn main() {{ let z = match Some(1) {{ Some(2) => f(-1), _ => 0 }}; print(z); }}"
    );
    let live = format!(
        "{PRELUDE} fn main() {{ let z = match Some(2) {{ Some(2) => f(-1), _ => 0 }}; print(z); }}"
    );
    assert!(!has_call_precondition(&dead));
    assert!(has_call_precondition(&live));
}

#[test]
fn exact_named_enum_payload_miss_skips_only_the_dead_value_arm() {
    let prelude = format!("enum E {{ V {{ a: i64 }} }}\n{PRELUDE}");
    let dead = format!(
        "{prelude} fn main() {{ let z = match E::V {{ a: 1 }} {{ E::V {{ a: 2 }} => f(-1), _ => 0 }}; print(z); }}"
    );
    let live = format!(
        "{prelude} fn main() {{ let z = match E::V {{ a: 2 }} {{ E::V {{ a: 2 }} => f(-1), _ => 0 }}; print(z); }}"
    );
    assert!(!has_call_precondition(&dead));
    assert!(has_call_precondition(&live));
}

#[test]
fn value_match_keeps_scrutinee_and_guard_calls_but_skips_dead_guard_body() {
    let scrutinee = format!(
        "{PRELUDE} fn main() {{ let z = match Some(f(-1)) {{ Some(2) => 0, _ => 0 }}; print(z); }}"
    );
    let guard = format!(
        "{PRELUDE} fn main() {{ let z = match Some(1) {{ Some(1) if f(-1) > 0 && false => 0, _ => 0 }}; print(z); }}"
    );
    let dead_body = format!(
        "{PRELUDE} fn main() {{ let z = match Some(1) {{ Some(1) if false => f(-1), _ => 0 }}; print(z); }}"
    );
    assert!(has_call_precondition(&scrutinee));
    assert!(has_call_precondition(&guard));
    assert!(!has_call_precondition(&dead_body));
}

#[test]
fn unknown_payload_remains_reachable_and_exact_match_is_terminal() {
    let unknown = format!(
        "{PRELUDE} fn main() {{ let p = 1; let z = match Some(p) {{ Some(2) => f(-1), _ => 0 }}; print(z); }}"
    );
    let terminal = format!(
        "{PRELUDE} fn main() {{ let z = match Some(1) {{ Some(1) => 0, _ => f(-1) }}; print(z); }}"
    );
    assert!(has_call_precondition(&unknown));
    assert!(!has_call_precondition(&terminal));
}
