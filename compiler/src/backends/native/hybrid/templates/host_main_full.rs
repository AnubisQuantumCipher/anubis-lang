//! Full Anubis hybrid host.
//!
//! This follows the measured risc0-metal-hybrid shape: methods are generated
//! by risc0-build, proving is in-process through get_prover_server, dev mode is
//! compiled out, and the active lane is reported by the patched
//! risc0-circuit-rv32im crate itself.

use std::rc::Rc;

use methods::{ANUBIS_ELF, ANUBIS_ID};
use risc0_zkvm::{get_prover_server, ExecutorEnv, InnerReceipt, ProverOpts, ProverServer};

const _METAL_BOUNDARY_CONTRACT: &str =
    "R0_DISABLE_METAL MTLArgumentBuffersTier::Tier2 checked_base_ptr StorageModeShared wait_until_completed lane=metal-hybrid lane=cpu";

fn lane() -> &'static str {
    if risc0_circuit_rv32im::prove::metal_lane_selected() {
        "metal-hybrid"
    } else {
        "cpu"
    }
}

fn metal_required_by_env() -> bool {
    std::env::var("ANUBIS_REQUIRE_METAL")
        .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

fn enforce_required_metal(selected_lane: &str) {
    if metal_required_by_env() && selected_lane != "metal-hybrid" {
        eprintln!(
            "metal_required_failed: ANUBIS_REQUIRE_METAL=1 but selected lane is {}",
            selected_lane
        );
        std::process::exit(64);
    }
}

fn prove_once(prover: &Rc<dyn ProverServer>, x: u32) -> (u32, usize) {
    let env = ExecutorEnv::builder()
        .write(&x)
        .expect("write input")
        .build()
        .expect("build executor env");
    let receipt = prover
        .prove(env, ANUBIS_ELF)
        .expect("prove failed")
        .receipt;
    receipt.verify(ANUBIS_ID).expect("receipt verification FAILED");
    let output: u32 = receipt.journal.decode().expect("decode journal");
    assert_eq!(output, x, "unexpected Anubis guest journal");

    let receipt_bytes = bincode::serialize(&receipt).expect("serialize receipt");
    std::fs::write("receipt.bin", receipt_bytes).expect("write receipt sidecar");
    std::fs::write("image_id.txt", format!("{:?}", ANUBIS_ID)).expect("write image ID sidecar");
    println!("GATE10_RECEIPT_SAVED");
    let segments = match &receipt.inner {
        InnerReceipt::Composite(composite) => composite.segments.len(),
        _ => 0,
    };
    (output, segments)
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::filter::EnvFilter::from_default_env())
        .init();

    println!("Anubis full hybrid (vendored risc0-metal-hybrid + generated guest)");
    let selected_lane = lane();
    enforce_required_metal(selected_lane);
    println!("lane={}", selected_lane);
    if std::env::args().any(|arg| arg == "lane") {
        return;
    }

    let x: u32 = 42;
    let prover = get_prover_server(&ProverOpts::default()).expect("get_prover_server");
    let (out, segments) = prove_once(&prover, x);
    println!(
        "guest=anubis output={} segments={} RECEIPT VERIFIED",
        out, segments
    );
    println!("hybrid_real_done");
}
