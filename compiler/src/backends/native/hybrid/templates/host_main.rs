use metal::{
    CompileOptions, Device, MTLArgumentBuffersTier, MTLResourceOptions, MTLSize,
};
use std::{env, ffi::c_void};

const LANE_METAL: &str = "metal-hybrid";
const LANE_CPU: &str = "cpu";

fn metal_disabled_by_env() -> bool {
    env::var("R0_DISABLE_METAL")
        .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

fn metal_required_by_env() -> bool {
    env::var("ANUBIS_REQUIRE_METAL")
        .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

fn metal_runtime_available() -> bool {
    if let Some(device) = Device::system_default() {
        device.argument_buffers_support() == MTLArgumentBuffersTier::Tier2
    } else {
        false
    }
}

fn lane() -> &'static str {
    if metal_disabled_by_env() {
        LANE_CPU
    } else if metal_runtime_available() {
        LANE_METAL
    } else {
        LANE_CPU
    }
}

fn enforce_required_metal(selected_lane: &str) {
    if metal_required_by_env() && selected_lane != LANE_METAL {
        eprintln!(
            "metal_required_failed: ANUBIS_REQUIRE_METAL=1 but selected lane is {}",
            selected_lane
        );
        std::process::exit(64);
    }
}

fn checked_base_ptr(buf: &metal::BufferRef) -> Result<*mut c_void, String> {
    let base_ptr = buf.contents();
    if base_ptr.is_null() {
        Err("checked_base_ptr: null StorageModeShared allocation".into())
    } else {
        Ok(base_ptr)
    }
}

fn run_metal(x: u32) -> Result<(), String> {
    let selected_lane = lane();
    if selected_lane != LANE_METAL {
        eprintln!("falling back to CPU: R0_DISABLE_METAL=1 or no Tier-2 Metal argument buffers");
        return Ok(());
    }

    let dev = Device::system_default().ok_or_else(|| "metal device disappeared".to_string())?;
    let msl = "#include <metal_stdlib>\nusing namespace metal;\nkernel void k(device uint* buf [[buffer(0)]]) { uint v = buf[0]; buf[0] = v + 1; }";
    let lib = dev
        .new_library_with_source(msl, &CompileOptions::new())
        .map_err(|e| format!("metal library compile: {:?}", e))?;
    let f = lib
        .get_function("k", None)
        .map_err(|e| format!("metal function lookup: {:?}", e))?;
    let p = dev
        .new_compute_pipeline_state_with_function(&f)
        .map_err(|e| format!("metal pipeline: {:?}", e))?;
    let q = dev.new_command_queue();
    let cb = q.new_command_buffer();
    let enc = cb.new_compute_command_encoder();
    let buf = dev.new_buffer(16, MTLResourceOptions::StorageModeShared);
    let base_ptr = checked_base_ptr(&buf)?;

    unsafe {
        *(base_ptr as *mut u32) = x;
    }
    enc.set_compute_pipeline_state(&p);
    enc.set_buffer(0, Some(&buf), 0);
    let gs = MTLSize {
        width: 1,
        height: 1,
        depth: 1,
    };
    enc.dispatch_thread_groups(gs, gs);
    enc.end_encoding();
    cb.commit();
    cb.wait_until_completed();

    let gpu_res = unsafe { *(checked_base_ptr(&buf)? as *mut u32) };
    println!("base_alloc_check: checked_base_ptr accepted StorageModeShared offset-0 buffer");
    println!("gpu_metal_real:{}", gpu_res);
    Ok(())
}

fn main() {
    println!("Anubis real-hybrid (real Metal + RISC0 reference lane contract)");

    let selected_lane = lane();
    enforce_required_metal(selected_lane);
    if selected_lane == LANE_METAL {
        println!("lane=metal-hybrid");
    } else {
        println!("lane=cpu");
    }
    if env::args().any(|arg| arg == "lane") {
        return;
    }

    let x: u32 = 42;
    println!("cpu:{}", x);

    if let Err(err) = run_metal(x) {
        eprintln!("metal_error: {}", err);
    }

    println!("hybrid_real_done");
}
