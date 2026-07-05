// Lowered from Anubis source AST (real walk, taint=raw: tainted<u32>|derived_from: raw|trace: raw -> sink|trace: raw -> declassify (declassified), constraints=2)
fn main() {
    println!("Anubis anubis_out artifact");
    println!("research_poc_triggered: true");
    println!("taint: raw: tainted<u32>|derived_from: raw|trace: raw -> sink|trace: raw -> declassify (declassified)");
    println!("constraints: 2");
    let raw: u32 = std::env::var("ANUBIS_TEST_RAW").ok().and_then(|s| s.parse().ok()).or_else(|| std::env::args().nth(1).and_then(|s| s.parse().ok())).unwrap_or(0);
    let write_idx = if raw < 1000 { raw as usize } else { 0 };
    println!("poc_memory_op_executed: wrote at idx {}", write_idx);
}
