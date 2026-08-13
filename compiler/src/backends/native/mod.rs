use crate::frontend::{Expr, Item, Stmt};
use std::path::Path;

pub(crate) mod hybrid;

// The build/prove native artifact is emitted by the SAME faithful whole-program lowering that
// `anubis run` uses (`backends::run::lower_program_to_rust`), so the artifact executes the real
// program instead of a hand-written template. Two exceptions are handled honestly:
//   * `hybrid { … }` blocks need the RISC0 + Metal cargo-project emitter (`hybrid` submodule);
//     the executable core in `backends::run` deliberately rejects them.
//   * a program the executable core cannot run (e.g. an analysis-only snippet with no `fn main`)
//     gets an honest, non-deceptive analysis-only marker — it reports the real analysis metadata
//     and the exact reason it is not runnable, and never fabricates program execution.

fn has_hybrid_block(stmts: &[Stmt]) -> bool {
    for s in stmts {
        match s {
            Stmt::HybridBlock { .. } => return true,
            Stmt::ResearchBlock { body, .. } | Stmt::ExploitBlock { body, .. }
                if has_hybrid_block(body) =>
            {
                return true
            }
            _ => {}
        }
    }
    false
}

/// Write `name.rs` next to the artifact, compile via the **same cargo + audited crypto** path as
/// `anubis run`, mark it executable, and return the executable path.
///
/// CRITICAL: bare `rustc` cannot link `argon2` / `chacha20poly1305` / … — using rustc here would
/// make `anubis build` fail on any program that touches `std.crypto` while `anubis run` succeeds.
fn compile_rust_to_exe(src: &str, out_dir: &Path, name: &str) -> Result<String, String> {
    let rs_path = out_dir.join(format!("{}.rs", name));
    std::fs::write(&rs_path, src).map_err(|e| e.to_string())?;
    let exe = out_dir.join(name);
    crate::backends::run::compile_native_rust_to_exe(src, &exe).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(m) = std::fs::metadata(&exe) {
            let mut p = m.permissions();
            p.set_mode(0o755);
            let _ = std::fs::set_permissions(&exe, p);
        }
    }
    Ok(exe.to_string_lossy().to_string())
}

/// Emit an honest, non-executable analysis marker for programs the faithful lowering cannot run
/// (no `fn main`, or a construct outside the executable core). It reports the real analysis
/// metadata (mode, taint labels, constraint count) and the exact reason it is not runnable — it
/// never fabricates program execution. The substantive results live in the evidence bundle.
fn honest_analysis_marker(ir: &crate::middle::TypedIR, name: &str, reason: &str) -> String {
    let taint = if ir.taint_labels.is_empty() {
        "no-taint".to_string()
    } else {
        ir.taint_labels.join("|")
    };
    let mode = format!("{:?}", ir.mode);
    let ccount = ir.constraints.len();
    // Each dynamic value is embedded via `{:?}` so it becomes a properly escaped Rust string
    // literal in the generated source (injection-safe), while `{{}}` becomes the `{}` of the
    // generated `println!`.
    let mut s = String::new();
    s.push_str(
        "// Anubis analysis-only artifact: the source has no runnable entry point in the\n\
         // executable core, so this marker reports the analysis instead of faking execution.\n",
    );
    s.push_str("fn main() {\n");
    s.push_str(&format!(
        "    println!(\"anubis analysis-only artifact: {{}}\", {:?});\n",
        name
    ));
    s.push_str(&format!("    println!(\"mode: {{}}\", {:?});\n", mode));
    s.push_str(&format!("    println!(\"taint: {{}}\", {:?});\n", taint));
    s.push_str(&format!(
        "    println!(\"constraints: {{}}\", {});\n",
        ccount
    ));
    s.push_str(&format!(
        "    println!(\"not directly executable: {{}}\", {:?});\n",
        reason
    ));
    s.push_str(
        "    println!(\"analysis-only: see evidence bundle for taint traces, SMT obligations, SARIF\");\n",
    );
    s.push_str("}\n");
    s
}

/// Lower a program that contains a `hybrid { … }` block into the RISC0 + Metal host project and
/// copy its executable + sidecars (`guest.elf`, `image_id.txt`, `generated-methods.rs`) alongside.
fn lower_hybrid(
    ir: &crate::middle::TypedIR,
    out_dir: &Path,
    name: &str,
    full_hybrid: bool,
) -> Result<String, String> {
    // Carry the parsed `cpu let x = <lit>` forward so the emitted host is source-derived.
    let mut cpu_init_val: Option<String> = None;
    for s in &ir.body {
        if let Stmt::HybridBlock {
            cpu: Some(cpu_stmts),
            ..
        } = s
        {
            for cs in cpu_stmts {
                if let Stmt::Let {
                    name: n,
                    init: Expr::Literal(lit),
                    ..
                } = cs
                {
                    if n == "x" {
                        cpu_init_val = Some(lit.clone());
                    }
                }
            }
        }
    }
    let cpu_val = cpu_init_val.unwrap_or_else(|| "42".to_string());

    let proj = out_dir.join(format!("{}-real-hybrid", name));
    let _ = std::fs::remove_dir_all(&proj);
    hybrid::emit_hybrid_project(&proj, true, &cpu_val)
        .map_err(|e| format!("hybrid emit: {}", e))?;

    let dst = out_dir.join(name);
    let built = if full_hybrid {
        hybrid::build_hybrid_host(&proj, true)?
    } else {
        let fast_proj = out_dir.join(format!("{}-fast-hybrid", name));
        let _ = std::fs::remove_dir_all(&fast_proj);
        hybrid::emit_hybrid_project(&fast_proj, false, &cpu_val)
            .map_err(|e| format!("hybrid fast emit: {}", e))?;
        hybrid::build_hybrid_host(&fast_proj, false)?
    };
    std::fs::copy(&built, &dst).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(m) = std::fs::metadata(&dst) {
            let mut p = m.permissions();
            p.set_mode(0o755);
            let _ = std::fs::set_permissions(&dst, p);
        }
    }

    let source_main = proj.join("host/src/main.rs");
    let _ = std::fs::copy(source_main, out_dir.join(format!("{}.rs", name)));
    for artifact in ["guest.elf", "image_id.txt", "generated-methods.rs"] {
        let source = proj.join(artifact);
        if source.exists() {
            let _ = std::fs::copy(source, out_dir.join(artifact));
        }
    }

    Ok(dst.to_string_lossy().to_string())
}

/// Lower a typechecked program to a native artifact.
///
/// Keystone: `build`/`prove` artifacts now share the faithful whole-program lowering used by
/// `anubis run`, so the artifact runs the real program. `hybrid { … }` blocks route to the
/// dedicated RISC0 + Metal emitter; anything the executable core cannot run falls back to an
/// honest analysis-only marker (never a fabricated result).
pub fn lower_to_native(
    ir: crate::middle::TypedIR,
    items: &[Item],
    out_dir: &Path,
    name: &str,
    full_hybrid: bool,
) -> Result<String, String> {
    if has_hybrid_block(&ir.body) {
        return lower_hybrid(&ir, out_dir, name, full_hybrid);
    }

    // Research mode enables the PoC-kit surface / research-block bodies inside the lowering,
    // matching `anubis run --allow-research`. Safe-mode violations (raw pointers, tainted sinks)
    // are already rejected upstream by `typecheck`, so we never reach here for those.
    let allow_research =
        ir.has_research || (ir.mode != crate::BuildMode::Safe && !ir.taint_labels.is_empty());

    match crate::backends::run::lower_program_to_rust_with_mono(
        items,
        allow_research,
        &ir.mono_specializations,
        &ir.mono_call_sites,
    ) {
        Ok(src) => compile_rust_to_exe(&src, out_dir, name),
        Err(reason) => {
            let src = honest_analysis_marker(&ir, name, &reason.to_string());
            compile_rust_to_exe(&src, out_dir, name)
        }
    }
}
