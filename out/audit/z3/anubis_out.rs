// Lowered from Anubis source AST (real walk, taint=x: tainted<u32>|derived_from: x, constraints=2)
fn main() {
    println!("Anubis anubis_out artifact");
    println!("research_poc_triggered: true");
    println!("taint: x: tainted<u32>|derived_from: x");
    println!("constraints: 2");
    let x: u32 = std::env::var("ANUBIS_TEST_X").ok().and_then(|s| s.parse().ok()).or_else(|| std::env::args().nth(1).and_then(|s| s.parse().ok())).unwrap_or(0);
    let write_idx = if x < 10 { x as usize } else { 0 };
    println!("poc_memory_op_executed: wrote at idx {}", write_idx);
}
