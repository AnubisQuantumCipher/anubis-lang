//! Cargo orchestration for hybrid projects. Returns the path to the built binary or Err.
//! No silent shim fallback.

use std::path::PathBuf;
use std::process::Command;

pub fn build_hybrid_host(proj_dir: &std::path::Path, full: bool) -> Result<PathBuf, String> {
    let status = Command::new("cargo")
        .args(["build", "--release"])
        .current_dir(proj_dir)
        .status()
        .map_err(|e| format!("host cargo spawn: {}", e))?;

    if !status.success() {
        return Err(format!(
            "cargo build --release failed for hybrid host project (full={}). See target/ logs.",
            full
        ));
    }

    let bin = proj_dir.join("target/release/anubis_hybrid_host");
    if !bin.exists() {
        return Err("cargo reported success but binary missing".into());
    }
    if full {
        export_generated_methods(proj_dir)?;
    }

    // Ensure executable bit
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&bin) {
            let mut p = meta.permissions();
            p.set_mode(0o755);
            let _ = std::fs::set_permissions(&bin, p);
        }
    }

    Ok(bin)
}

fn export_generated_methods(proj_dir: &std::path::Path) -> Result<(), String> {
    let methods_rs = find_generated_methods_rs(&proj_dir.join("target/release/build"))
        .or_else(|| find_generated_methods_rs(&proj_dir.join("target/debug/build")))
        .ok_or_else(|| {
            "risc0-build succeeded but generated methods.rs was not found".to_string()
        })?;
    let methods_text = std::fs::read_to_string(&methods_rs)
        .map_err(|e| format!("read generated methods.rs: {}", e))?;
    if !methods_text.contains("ANUBIS_ELF") || !methods_text.contains("ANUBIS_ID") {
        return Err("generated methods.rs is missing ANUBIS_ELF/ANUBIS_ID".into());
    }
    std::fs::copy(&methods_rs, proj_dir.join("generated-methods.rs"))
        .map_err(|e| format!("copy generated methods.rs: {}", e))?;

    let image_id = extract_image_id(&methods_text)
        .ok_or_else(|| "generated ANUBIS_ID could not be parsed".to_string())?;
    std::fs::write(proj_dir.join("image_id.txt"), image_id.join(" "))
        .map_err(|e| format!("write image_id.txt: {}", e))?;

    if let Some(guest_elf) = extract_guest_elf_path(&methods_text, &methods_rs) {
        if guest_elf.exists() {
            std::fs::copy(&guest_elf, proj_dir.join("guest.elf"))
                .map_err(|e| format!("copy guest.elf: {}", e))?;
        }
    }
    if !proj_dir.join("guest.elf").exists() {
        return Err(
            "generated ANUBIS_ELF exists but physical guest ELF could not be exported".into(),
        );
    }
    Ok(())
}

fn find_generated_methods_rs(root: &std::path::Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path().join("out/methods.rs");
        if path.exists() {
            return Some(path);
        }
    }
    None
}

fn extract_image_id(methods_text: &str) -> Option<Vec<String>> {
    let id_pos = methods_text.find("ANUBIS_ID")?;
    let after_id = &methods_text[id_pos..];
    let eq = after_id.find('=')?;
    let after_eq = &after_id[eq + 1..];
    let start = after_eq.find('[')?;
    let end = after_eq[start + 1..].find(']')? + start + 1;
    let words = after_eq[start + 1..end]
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    (words.len() == 8).then_some(words)
}

fn extract_guest_elf_path(methods_text: &str, methods_rs: &std::path::Path) -> Option<PathBuf> {
    let elf_pos = methods_text.find("ANUBIS_ELF")?;
    let after_elf = &methods_text[elf_pos..];
    let include_pos = after_elf.find("include_bytes!")?;
    let after_include = &after_elf[include_pos..];
    let first_quote = after_include.find('"')?;
    let rest = &after_include[first_quote + 1..];
    let second_quote = rest.find('"')?;
    let raw_path = &rest[..second_quote];
    let path = std::path::Path::new(raw_path);
    if path.is_absolute() {
        Some(path.to_path_buf())
    } else {
        methods_rs.parent().map(|parent| parent.join(path))
    }
}

#[cfg(test)]
mod tests {
    use super::extract_image_id;

    #[test]
    fn extracts_generated_anubis_id_after_type_annotation() {
        let generated = r#"
pub const ANUBIS_ELF: &[u8] = include_bytes!("/tmp/anubis.bin");
pub const ANUBIS_ID: [u32; 8] = [586004394, 518297615, 348739185, 1975091652, 3647604622, 2354757258, 4191699605, 2772140320];
"#;

        assert_eq!(
            extract_image_id(generated),
            Some(vec![
                "586004394".to_string(),
                "518297615".to_string(),
                "348739185".to_string(),
                "1975091652".to_string(),
                "3647604622".to_string(),
                "2354757258".to_string(),
                "4191699605".to_string(),
                "2772140320".to_string(),
            ])
        );
    }
}
