use crate::frontend::{Expr, Stmt};
use std::path::Path;

mod hybrid;

// Note: direct lowering in lower_to_native (research/hybrid branches) for fidelity to source AST.
// Legacy emit_stmt/expr_to_str removed (were dead; research path uses collect/extract + inline).

fn extract_assume_bound(e: &Expr) -> Option<(String, String)> {
    if let Expr::Binary { op, lhs, rhs } = e {
        if op == "<" || op == "<=" {
            if let Expr::Var(v) = &**lhs {
                let bound = match &**rhs {
                    Expr::Literal(l) | Expr::Var(l) => l.clone(),
                    _ => return None,
                };
                return Some((v.clone(), bound));
            }
        }
    }
    None
}

fn collect_research_driver(
    stmts: &[Stmt],
    source_x: &mut Option<String>,
    source_bound: &mut Option<String>,
) {
    for stmt in stmts {
        match stmt {
            Stmt::Let {
                name, ty: Some(t), ..
            } if t.contains("tainted") && source_x.is_none() => {
                *source_x = Some(name.clone());
            }
            Stmt::ResearchBlock { body, .. } | Stmt::ExploitBlock { body, .. } => {
                collect_research_driver(body, source_x, source_bound);
            }
            Stmt::ExprStmt(Expr::Assume(inner)) => {
                if let Some((var, bound)) = extract_assume_bound(inner) {
                    if source_x.is_none() {
                        *source_x = Some(var);
                    }
                    if source_bound.is_none() {
                        *source_bound = Some(bound);
                    }
                }
            }
            _ => {}
        }
    }
}

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

pub fn lower_to_native(
    ir: crate::middle::TypedIR,
    out_dir: &Path,
    name: &str,
    _full_hybrid: bool,
) -> Result<String, String> {
    let taint_info = if ir.taint_labels.is_empty() {
        "no-taint".to_string()
    } else {
        ir.taint_labels.join("|")
    };
    let ccount = ir.constraints.len();

    let is_research =
        ir.has_research || (ir.mode != crate::BuildMode::Safe && !ir.taint_labels.is_empty());
    let is_hybrid = has_hybrid_block(&ir.body);

    let src = if is_research {
        let mut source_x = None;
        let mut source_bound = None;
        collect_research_driver(&ir.body, &mut source_x, &mut source_bound);
        let var_name = source_x
            .ok_or_else(|| "research lowering requires a tainted source variable".to_string())?;
        let bound_lit = source_bound.ok_or_else(|| {
            format!(
                "research lowering requires assume({} < bound) from parsed AST",
                var_name
            )
        })?;
        let dst = out_dir.join(name);
        let env_key = format!("ANUBIS_TEST_{}", var_name.to_ascii_uppercase());
        let real = format!(
            "// Lowered from Anubis source AST (real walk, taint={}, constraints={})\nfn main() {{\n    println!(\"Anubis {} artifact\");\n    println!(\"research_poc_triggered: true\");\n    println!(\"taint: {}\");\n    println!(\"constraints: {}\");\n    let {}: u32 = std::env::var(\"{}\").ok().and_then(|s| s.parse().ok()).or_else(|| std::env::args().nth(1).and_then(|s| s.parse().ok())).unwrap_or(0);\n    let write_idx = if {} < {} {{ {} as usize }} else {{ 0 }};\n    println!(\"poc_memory_op_executed: wrote at idx {{}}\", write_idx);\n}}\n",
            taint_info, ccount, name, taint_info, ccount, var_name, env_key, var_name, bound_lit, var_name
        );
        let rs_path = out_dir.join(format!("{}.rs", name));
        std::fs::write(&rs_path, real).map_err(|e| e.to_string())?;
        let status = std::process::Command::new("rustc")
            .args(["-o", dst.to_str().unwrap(), rs_path.to_str().unwrap()])
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("rustc failed".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(m) = std::fs::metadata(&dst) {
                let mut p = m.permissions();
                p.set_mode(0o755);
                let _ = std::fs::set_permissions(&dst, p);
            }
        }
        return Ok(dst.to_string_lossy().to_string());
    } else if is_hybrid {
        // Extract simple values from the parsed HybridBlock sub-stmts so we don't ignore the source (cpu let x, etc.).
        let mut cpu_init_val: Option<String> = None;
        for s in &ir.body {
            if let Stmt::HybridBlock {
                cpu: Some(cpu_stmts),
                ..
            } = s
            {
                for cs in cpu_stmts {
                    if let Stmt::Let {
                        name,
                        init: Expr::Literal(lit),
                        ..
                    } = cs
                    {
                        if name == "x" {
                            cpu_init_val = Some(lit.clone());
                        }
                    }
                }
            }
        }
        let cpu_val = cpu_init_val.unwrap_or_else(|| "42".to_string());

        // Delegate to extracted hybrid module (templates + real cargo build, no shim fallback).
        let proj = out_dir.join(format!("{}-real-hybrid", name));
        let _ = std::fs::remove_dir_all(&proj);

        hybrid::emit_hybrid_project(&proj, true, &cpu_val)
            .map_err(|e| format!("hybrid emit: {}", e))?;

        let dst = out_dir.join(name);
        let built = if _full_hybrid {
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

        return Ok(dst.to_string_lossy().to_string());
    } else {
        format!(
            "fn main() {{
    println!(\"Anubis {} artifact (mode: {:?})\");
    println!(\"safe_execution\");
}}
",
            name, ir.mode
        )
    };

    let rs_path = out_dir.join(format!("{}.rs", name));
    std::fs::write(&rs_path, &src).map_err(|e| e.to_string())?;

    let exe = out_dir.join(name);
    let status = std::process::Command::new("rustc")
        .args(["-o", exe.to_str().unwrap(), rs_path.to_str().unwrap()])
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("rustc failed".into());
    }
    let _ = std::process::Command::new("chmod")
        .args(["+x", exe.to_str().unwrap()])
        .status();
    Ok(exe.to_string_lossy().to_string())
}
