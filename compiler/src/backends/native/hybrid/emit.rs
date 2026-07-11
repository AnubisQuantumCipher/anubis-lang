//! Pure emission for hybrid projects. Uses checked templates (no hand-written push_str of API code).
//! Fast mode: metal-only (real dispatch). Full mode adds risc0.

use std::fs;
use std::io;
use std::path::Path;

pub fn emit_hybrid_project(proj_dir: &Path, _full: bool, cpu_val: &str) -> Result<(), String> {
    fs::create_dir_all(proj_dir).map_err(|e| e.to_string())?;
    let cargo = if _full {
        include_str!("templates/Cargo.full.toml")
    } else {
        include_str!("templates/Cargo.fast.toml")
    };
    fs::write(proj_dir.join("Cargo.toml"), cargo).map_err(|e| e.to_string())?;

    if _full {
        emit_full_hybrid_project(proj_dir, cpu_val)?;
    } else {
        emit_fast_hybrid_project(proj_dir, cpu_val)?;
    }

    let readme = if _full {
        "Anubis full hybrid host project.\n\nShape: vendored risc0-metal-hybrid patch + risc0-build methods crate + generated guest ELF/image ID + stock receipt.verify(ANUBIS_ID).\nBuild: cargo build --release\nRun: ./target/release/anubis_hybrid_host\nCPU lane: R0_DISABLE_METAL=1 ./target/release/anubis_hybrid_host\n"
    } else {
        "Anubis fast hybrid host project.\n\nShape: real Metal dispatch lane probe + CPU fallback, without RISC0 proof generation.\nBuild: cargo build --release\nRun: ./target/release/anubis_hybrid_host\n"
    };
    fs::write(proj_dir.join("README.md"), readme).map_err(|e| e.to_string())?;

    Ok(())
}

fn emit_fast_hybrid_project(proj_dir: &Path, cpu_val: &str) -> Result<(), String> {
    fs::create_dir_all(proj_dir.join("src")).map_err(|e| e.to_string())?;
    let main_rs = include_str!("templates/host_main.rs")
        .replace("let x: u32 = 42;", &format!("let x: u32 = {};", cpu_val));
    fs::write(proj_dir.join("src/main.rs"), main_rs).map_err(|e| e.to_string())?;
    Ok(())
}

fn emit_full_hybrid_project(proj_dir: &Path, cpu_val: &str) -> Result<(), String> {
    fs::create_dir_all(proj_dir.join("host/src")).map_err(|e| e.to_string())?;
    fs::create_dir_all(proj_dir.join("methods/src")).map_err(|e| e.to_string())?;
    fs::create_dir_all(proj_dir.join("methods/guest/src")).map_err(|e| e.to_string())?;

    fs::write(
        proj_dir.join("host/Cargo.toml"),
        include_str!("templates/host_Cargo.full.toml"),
    )
    .map_err(|e| e.to_string())?;
    let main_rs = include_str!("templates/host_main_full.rs")
        .replace("let x: u32 = 42;", &format!("let x: u32 = {};", cpu_val));
    fs::write(proj_dir.join("host/src/main.rs"), main_rs).map_err(|e| e.to_string())?;

    fs::write(
        proj_dir.join("methods/Cargo.toml"),
        include_str!("templates/methods_Cargo.toml"),
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        proj_dir.join("methods/build.rs"),
        include_str!("templates/methods_build.rs"),
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        proj_dir.join("methods/src/lib.rs"),
        include_str!("templates/methods_lib.rs"),
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        proj_dir.join("methods/guest/Cargo.toml"),
        include_str!("templates/guest_Cargo.toml"),
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        proj_dir.join("methods/guest/src/main.rs"),
        include_str!("templates/guest_main.rs"),
    )
    .map_err(|e| e.to_string())?;

    let base = std::env::var("ANUBIS_RISC0_METAL_REFERENCE")
        .unwrap_or_else(|_| "/tmp/test-metal-prover".to_string());
    let vendored_src_buf = std::path::PathBuf::from(&base).join("vendor/risc0-circuit-rv32im");
    let vendored_src = vendored_src_buf.as_path();
    let vendored_dst = proj_dir.join("vendor/risc0-circuit-rv32im");
    if !vendored_src.join("src/prove/hal/metal.rs").exists() {
        return Err(format!(
            "vendored risc0-metal-hybrid crate missing at {}",
            vendored_src.display()
        ));
    }
    copy_dir_recursive(vendored_src, &vendored_dst)?;
    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    if dst.exists() {
        fs::remove_dir_all(dst).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let source_path = entry.path();
        let target_path = dst.join(entry.file_name());
        let file_type = entry.file_type().map_err(|e| e.to_string())?;
        if file_type.is_dir() {
            if entry.file_name() == "target" {
                continue;
            }
            copy_dir_recursive(&source_path, &target_path)?;
        } else if file_type.is_file() {
            copy_file_buffered(&source_path, &target_path)?;
        }
    }
    Ok(())
}

fn copy_file_buffered(src: &Path, dst: &Path) -> Result<(), String> {
    let mut reader = fs::File::open(src).map_err(|e| e.to_string())?;
    let mut writer = fs::File::create(dst).map_err(|e| e.to_string())?;
    io::copy(&mut reader, &mut writer).map_err(|e| e.to_string())?;
    Ok(())
}
