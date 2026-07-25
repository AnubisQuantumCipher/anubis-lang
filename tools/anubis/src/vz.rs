//! `anubis vz` — the virtualization lifecycle, first-class in the Anubis toolchain.
//!
//! Anubis is a high-assurance systems language; a language that can PROVE things about code is far more
//! useful when it can also run that code in an isolated, reproducible virtual machine on the same
//! hardware it targets (Apple Silicon). This subcommand gives the complete VM lifecycle — create, boot,
//! introspect, exec into, snapshot-by-clone, stop, delete — behind one CLI, so an operator never leaves
//! the `anubis` tool to stand up a sealed environment (the exact pattern the project's own VM-seal
//! battery uses).
//!
//! HONEST IMPLEMENTATION NOTE. Most commands here drive Apple Virtualization.framework guests through
//! `tart` (Cirrus Labs' Virtualization.framework wrapper) — the same VZ layer the repo's
//! `scripts/vm/run-slice.sh` already relies on. `tart` owns the entitlement + code-signing for the
//! full guest lifecycle, so wrapping it keeps that integration real and testable. The `native-preflight`
//! command (see `vz_native.rs`) is the NATIVE `objc2-virtualization` backend — NO `tart` — and it
//! enforces the two confinements tart cannot (a true zero-NIC air-gap, a per-hostname egress
//! substrate). Its `com.apple.security.virtualization` entitlement is NOT a Developer-portal step: a
//! LOCAL ad-hoc signature (`scripts/build_signed_anubis.sh`) is sufficient to run it on your own Mac —
//! only notarization-for-distribution is a human step. So the native lane is no longer `[NEEDS-HUMAN]`
//! for local use. Every `tart`-backed command below is a thin, auditable shell over `tart` — no hidden
//! state.

use anyhow::{anyhow, bail, Context, Result};
use clap::Subcommand;
use std::ffi::OsString;
use std::path::Path;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(Subcommand, Debug)]
pub enum VzCmd {
    /// Report virtualization readiness: the VZ backend (tart), the host architecture, and current VMs.
    Status {
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// List the virtual machines known to the VZ backend.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Create a VM by cloning a base image (an OCI image ref or a local VM name).
    Create {
        /// Name for the new VM.
        name: String,
        /// Base image to clone from (e.g. `ghcr.io/cirruslabs/macos-sonoma-base:latest` or a local VM).
        #[arg(long)]
        from: String,
        /// vCPU count to pin (optional).
        #[arg(long)]
        cpu: Option<u32>,
        /// Memory in MiB to pin (optional).
        #[arg(long)]
        memory: Option<u32>,
    },
    /// Boot a VM. Runs headless by default; blocks until the guest stops (use `--detach` to background).
    Run {
        name: String,
        /// Run without a graphical window (headless).
        #[arg(long, default_value_t = true)]
        no_graphics: bool,
        /// Detach — boot in the background and return immediately.
        #[arg(long)]
        detach: bool,
        /// Bind-mount a host directory into the guest (`--dir name:/host/path`), repeatable.
        #[arg(long)]
        dir: Vec<String>,
        /// Hostname allow-list for net.send apply (staged DNS-pinned policy; tart stays host-only).
        #[arg(long = "allow-host")]
        allow_host: Vec<String>,
        /// Explicit residual: allow full tart NAT when net.send is proved (default is host-only).
        #[arg(long, default_value_t = false)]
        allow_open_nat: bool,
        /// Slice-2: derive confinement from this program and APPLY tart args (e.g. `--net-host`)
        /// to the live boot. Writes `confinement_applied.json` (or `--applied-out`).
        #[arg(long)]
        confine: Option<String>,
        /// Where to write the applied confinement artifact (default: ./confinement_applied.json).
        #[arg(long)]
        applied_out: Option<String>,
    },
    /// Slice-2: derive confinement and emit the applied artifact WITHOUT booting (dry-run apply).
    Apply {
        /// Program to derive confinement from.
        program: String,
        /// Optional tart VM name — if set and tart is available, also boots with applied flags.
        #[arg(long)]
        name: Option<String>,
        #[arg(long, default_value_t = true)]
        no_graphics: bool,
        #[arg(long)]
        detach: bool,
        #[arg(long)]
        dir: Vec<String>,
        /// Hostname allow-list (staged); requires proved net.send.
        #[arg(long = "allow-host")]
        allow_host: Vec<String>,
        /// Explicit residual open NAT when net.send proved.
        #[arg(long, default_value_t = false)]
        allow_open_nat: bool,
        #[arg(long)]
        applied_out: Option<String>,
    },
    /// Print the IP address of a running VM.
    Ip { name: String },
    /// Run a command inside a running VM over SSH.
    Exec {
        name: String,
        /// SSH user in the guest.
        #[arg(long, default_value = "admin")]
        user: String,
        /// The command (and args) to run, after `--`.
        #[arg(last = true, required = true)]
        command: Vec<String>,
    },
    /// Snapshot a VM by cloning it to a new name (VZ has no in-place snapshots; a clone is CoW-cheap).
    Snapshot {
        name: String,
        /// Name of the snapshot clone.
        to: String,
    },
    /// Stop a running VM.
    Stop { name: String },
    /// Delete a VM.
    Delete {
        name: String,
        /// Do not prompt.
        #[arg(long)]
        force: bool,
    },
    /// rsync a host directory INTO a running guest (`--from ./x --to /Users/admin/x`).
    Sync {
        name: String,
        #[arg(long, default_value = ".")]
        from: String,
        #[arg(long)]
        to: String,
        #[arg(long, default_value = "admin")]
        user: String,
    },
    /// Run an Anubis PoC in a DISPOSABLE guest — clone → boot → sync PoC → `anubis run --allow-research`
    /// inside → discard. Blast radius is the throwaway VM, never the host. Requires `--allow-research`.
    Exploit {
        /// The PoC `.anb` (relative to the host cwd).
        poc: String,
        /// Base image/VM to clone the disposable guest from (must have the anubis toolchain).
        #[arg(long, default_value = "anubis-xcode")]
        base: String,
        /// Keep the guest afterwards for inspection instead of deleting it.
        #[arg(long)]
        keep: bool,
        /// Acknowledge that this runs offensive code (gated, like every dangerous Anubis operation).
        #[arg(long)]
        allow_research: bool,
        #[arg(long, default_value = "admin")]
        user: String,
    },
    /// Derive the hypervisor CONFINEMENT policy from a program's PROVEN capability set (the six
    /// canonical effects), and print it as JSON. Fails closed: a program that does not pass
    /// `anubis check` has no proof to derive confinement from, so it is refused. The same manifest is
    /// sealed + re-derivable in every `anubis build --evidence` bundle. This never boots a VM;
    /// APPLYING the derived flags is `vz run --confine` / `vz apply` (slice-2).
    Confine {
        /// The program `.anb` to derive confinement for.
        program: String,
        /// Write the manifest JSON to this path instead of stdout.
        #[arg(long)]
        out: Option<String>,
    },
    /// NATIVE backend (no tart): derive the hardware confinement posture from a program's PROVEN
    /// effect set, build the exact `VZVirtualMachineConfiguration` it implies via
    /// `objc2-virtualization`, and prove the `com.apple.security.virtualization` entitlement is
    /// present by validating + instantiating it. Enforces the two confinements tart cannot: a true
    /// zero-NIC air-gap (net-free programs) and a per-hostname egress substrate (net-using programs).
    /// Never boots a guest. Requires a binary signed with `scripts/build_signed_anubis.sh`.
    NativePreflight {
        /// The program `.anb` to derive the native confinement for.
        program: String,
        /// Allow-list a hostname for egress (repeatable). Only meaningful for a net-using program.
        #[arg(long = "allow-host")]
        allow_host: Vec<String>,
    },
    /// NATIVE boot: same posture as native-preflight, with a real kernel (+ optional initrd).
    /// Spawns the DNS-pinned egress frame pump for net-using programs. Requires signed binary.
    NativeBoot {
        program: String,
        /// Path to a Linux kernel image for VZLinuxBootLoader.
        #[arg(long)]
        kernel: String,
        /// Optional initrd path.
        #[arg(long)]
        initrd: Option<String>,
        #[arg(long = "allow-host")]
        allow_host: Vec<String>,
    },
    /// Fuzz a target in a DISPOSABLE guest (clone → boot → sync → `anubis fuzz` inside → discard).
    Fuzz {
        target: String,
        #[arg(long, default_value_t = 1000)]
        iterations: u64,
        #[arg(long, default_value = "anubis-xcode")]
        base: String,
        #[arg(long)]
        keep: bool,
        #[arg(long)]
        allow_research: bool,
        #[arg(long, default_value = "admin")]
        user: String,
    },
}

/// Locate the VZ backend (`tart`) or fail with an actionable message.
fn tart_bin() -> Result<&'static str> {
    let ok = Command::new("tart")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok {
        Ok("tart")
    } else {
        Err(anyhow!(
            "ANUBIS_VZ_BACKEND_MISSING: the VZ backend `tart` (Apple Virtualization.framework wrapper) \
             is not installed or not on PATH. Install it with `brew install cirruslabs/cli/tart`. \
             Anubis VZ requires Apple Silicon macOS."
        ))
    }
}

/// Run `tart <args>`, streaming stdout/stderr, and map a non-zero exit to an error.
fn tart_run(args: &[String]) -> Result<()> {
    let bin = tart_bin()?;
    let status = Command::new(bin)
        .args(args)
        .status()
        .with_context(|| format!("failed to spawn `{bin} {}`", args.join(" ")))?;
    if status.success() {
        Ok(())
    } else {
        bail!(
            "ANUBIS_VZ_COMMAND_FAILED: `tart {}` exited with {}",
            args.join(" "),
            status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "signal".into())
        )
    }
}

/// Run `tart <args>` and capture stdout (trimmed).
fn tart_capture(args: &[String]) -> Result<String> {
    let bin = tart_bin()?;
    let out = Command::new(bin)
        .args(args)
        .output()
        .with_context(|| format!("failed to spawn `{bin} {}`", args.join(" ")))?;
    if !out.status.success() {
        bail!(
            "ANUBIS_VZ_COMMAND_FAILED: `tart {}` exited non-zero: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn s(v: &str) -> String {
    v.to_string()
}

pub fn run_vz_cmd(action: VzCmd) -> Result<()> {
    match action {
        VzCmd::Status { json } => {
            let backend_ok = tart_bin().is_ok();
            let arch = std::env::consts::ARCH;
            let apple_silicon = arch == "aarch64" && cfg!(target_os = "macos");
            let vms = if backend_ok {
                tart_capture(&[s("list")]).unwrap_or_default()
            } else {
                String::new()
            };
            if json {
                println!(
                    "{{\"backend\":\"tart\",\"backend_available\":{backend_ok},\"arch\":\"{arch}\",\"apple_silicon\":{apple_silicon},\"virtualization_framework\":{apple_silicon}}}"
                );
            } else {
                println!("anubis vz — virtualization status");
                println!(
                    "  backend            : tart (Apple Virtualization.framework)  [{}]",
                    if backend_ok {
                        "available"
                    } else {
                        "MISSING — brew install cirruslabs/cli/tart"
                    }
                );
                println!("  host arch          : {arch}");
                println!(
                    "  Virtualization.fwk : {}",
                    if apple_silicon {
                        "supported (Apple Silicon macOS)"
                    } else {
                        "NOT available on this host"
                    }
                );
                if backend_ok {
                    println!("\n  virtual machines:\n{}", indent(&vms));
                }
            }
            Ok(())
        }
        VzCmd::List { json } => {
            let out = tart_capture(&[s("list")])?;
            if json {
                // tart list has a --format json; pass through when asked.
                let j = tart_capture(&[s("list"), s("--format"), s("json")]).unwrap_or(out);
                println!("{j}");
            } else {
                println!("{out}");
            }
            Ok(())
        }
        VzCmd::Create {
            name,
            from,
            cpu,
            memory,
        } => {
            eprintln!("[anubis vz] cloning `{from}` -> `{name}`");
            tart_run(&[s("clone"), from, name.clone()])?;
            if cpu.is_some() || memory.is_some() {
                let mut set = vec![s("set"), name.clone()];
                if let Some(c) = cpu {
                    set.push(s("--cpu"));
                    set.push(c.to_string());
                }
                if let Some(m) = memory {
                    set.push(s("--memory"));
                    set.push(m.to_string());
                }
                tart_run(&set)?;
            }
            println!("created VM `{name}` (run it with `anubis vz run {name}`)");
            Ok(())
        }
        VzCmd::Run {
            name,
            no_graphics,
            detach,
            dir,
            allow_host,
            allow_open_nat,
            confine,
            applied_out,
        } => {
            let mut confine_args: Vec<String> = Vec::new();
            // When --confine is set, mounts/network are posture-filtered (fail-closed).
            let mut run_mounts: Vec<String> = dir.clone();
            if let Some(prog) = confine {
                let eng = crate::vz_apply::ApplyEngagement {
                    mounts: dir.clone(),
                    allow_hosts: allow_host.clone(),
                    allow_open_nat,
                };
                let (applied, _) = crate::vz_apply::build_applied(&prog, &eng)?;
                let out_path = applied_out
                    .as_ref()
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|| std::path::PathBuf::from(crate::vz_apply::APPLIED_FILENAME));
                crate::vz_apply::write_applied(&applied, Some(&out_path))?;
                eprintln!(
                    "[anubis vz run --confine] mode={} tart_args=[{}] mount_posture={} mounts={} → {}",
                    applied.network_apply_mode,
                    applied.tart_args.join(" "),
                    applied.mount_posture,
                    applied.mounts.len(),
                    out_path.display()
                );
                confine_args = applied.tart_args;
                run_mounts = applied.mounts;
            }
            let mut args = vec![s("run"), name.clone()];
            if no_graphics {
                args.push(s("--no-graphics"));
            }
            for a in &confine_args {
                args.push(a.clone());
            }
            for d in &run_mounts {
                args.push(s("--dir"));
                args.push(d.clone());
            }
            if detach {
                // Background the guest; tart keeps it running until `vz stop`.
                let bin = tart_bin()?;
                Command::new(bin)
                    .args(&args)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .with_context(|| "failed to detach the VZ guest")?;
                println!("booting `{name}` in the background (headless). `anubis vz ip {name}` when ready.");
                Ok(())
            } else {
                eprintln!(
                    "[anubis vz] booting `{name}` (Ctrl-C to stop); the guest holds this terminal."
                );
                tart_run(&args)
            }
        }
        VzCmd::Apply {
            program,
            name,
            no_graphics,
            detach,
            dir,
            allow_host,
            allow_open_nat,
            applied_out,
        } => {
            let eng = crate::vz_apply::ApplyEngagement {
                mounts: dir.clone(),
                allow_hosts: allow_host.clone(),
                allow_open_nat,
            };
            let (applied, _) = crate::vz_apply::build_applied(&program, &eng)?;
            let out_path = applied_out
                .as_ref()
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::path::PathBuf::from(crate::vz_apply::APPLIED_FILENAME));
            crate::vz_apply::write_applied(&applied, Some(&out_path))?;
            println!("{}", serde_json::to_string_pretty(&applied)?);
            eprintln!("[anubis vz apply] wrote {}", out_path.display());
            if let Some(vm) = name {
                crate::vz_apply::apply_and_run(
                    &program,
                    &vm,
                    &eng,
                    no_graphics,
                    detach,
                    Some(&out_path),
                )?;
            }
            Ok(())
        }
        VzCmd::Ip { name } => {
            let ip = tart_capture(&[s("ip"), name])?;
            println!("{ip}");
            Ok(())
        }
        VzCmd::Exec {
            name,
            user,
            command,
        } => {
            let ip = tart_capture(&[s("ip"), name.clone()]).with_context(|| {
                format!("VM `{name}` has no IP — is it running? (`anubis vz run {name} --detach`)")
            })?;
            let target = format!("{user}@{ip}");
            eprintln!("[anubis vz] ssh {target} -- {}", command.join(" "));
            let status = Command::new("ssh")
                .args(ssh_common_args()?)
                .arg(&target)
                .args(&command)
                .status()
                .with_context(|| "failed to spawn ssh")?;
            if status.success() {
                Ok(())
            } else {
                bail!(
                    "ANUBIS_VZ_EXEC_FAILED: remote command exited with {}",
                    status
                        .code()
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| "signal".into())
                )
            }
        }
        VzCmd::Snapshot { name, to } => {
            eprintln!("[anubis vz] snapshotting `{name}` -> `{to}` (APFS CoW clone)");
            tart_run(&[s("clone"), name, to.clone()])?;
            println!("snapshot `{to}` created");
            Ok(())
        }
        VzCmd::Stop { name } => tart_run(&[s("stop"), name]),
        VzCmd::Delete { name, force } => {
            if !force {
                eprintln!("[anubis vz] deleting `{name}` (pass --force to skip this notice)");
            }
            tart_run(&[s("delete"), name])
        }
        VzCmd::Sync {
            name,
            from,
            to,
            user,
        } => {
            let ip = wait_for_ip(&name)?;
            rsync_into(&from, &format!("{user}@{ip}:{to}"))
        }
        VzCmd::Exploit {
            poc,
            base,
            keep,
            allow_research,
            user,
        } => {
            if !allow_research {
                bail!(
                    "ANUBIS_VZ_RESEARCH_REQUIRED: `anubis vz exploit` runs offensive code — pass \
                     --allow-research to acknowledge. The blast radius is the disposable guest, not the host."
                );
            }
            disposable(&base, keep, |name, ip| {
                let dst = format!("{user}@{ip}:/tmp/anubis-poc.anb");
                rsync_into(&poc, &dst)?;
                // Gold PoC-kit oracle (path used by examples/security/poc_local_overflow.anb).
                // Without this, disposable guests only receive the .anb + runner and spawn fails.
                let gold = Path::new("poc_kit/bin/vuln_local");
                if gold.is_file() {
                    ssh_exec(
                        user.clone(),
                        ip.clone(),
                        &["mkdir".into(), "-p".into(), "poc_kit/bin".into()],
                    )?;
                    rsync_into(
                        gold.to_str().ok_or_else(|| {
                            anyhow!("ANUBIS_VZ_SYNC_FAILED: gold vuln path is not UTF-8")
                        })?,
                        &format!("{user}@{ip}:poc_kit/bin/vuln_local"),
                    )?;
                }
                let runner = sync_current_anubis(&user, &ip)?;
                eprintln!("[anubis vz] running `anubis run /tmp/anubis-poc.anb --allow-research` in disposable `{name}`");
                // `$HOME` so relative `poc_kit/bin/vuln_local` resolves after stage.
                let remote = format!(
                    "cd \"$HOME\" && env ANUBIS_VZ_GUEST=1 ANUBIS_OFFENSIVE_GATE_IN_GUEST=1 \
                     ANUBIS_ISOLATION=tart-disposable-guest {runner} run /tmp/anubis-poc.anb --allow-research"
                );
                ssh_exec(user, ip, &["bash".into(), "-lc".into(), remote])
            })
        }
        VzCmd::Confine { program, out } => run_confine(&program, out),
        VzCmd::NativePreflight {
            program,
            allow_host,
        } => crate::vz_native::native_preflight(&program, &allow_host),
        VzCmd::NativeBoot {
            program,
            kernel,
            initrd,
            allow_host,
        } => crate::vz_native::native_boot(&program, &kernel, initrd.as_deref(), &allow_host),
        VzCmd::Fuzz {
            target,
            iterations,
            base,
            keep,
            allow_research,
            user,
        } => {
            if !allow_research {
                bail!("ANUBIS_VZ_RESEARCH_REQUIRED: `anubis vz fuzz` runs offensive code — pass --allow-research.");
            }
            disposable(&base, keep, |name, ip| {
                let dst = format!("{user}@{ip}:/tmp/anubis-fuzz.anb");
                rsync_into(&target, &dst)?;
                let runner = sync_current_anubis(&user, &ip)?;
                eprintln!("[anubis vz] fuzzing in disposable `{name}` ({iterations} iterations)");
                ssh_exec(
                    user,
                    ip,
                    &[
                        "env".into(),
                        "ANUBIS_VZ_GUEST=1".into(),
                        "ANUBIS_OFFENSIVE_GATE_IN_GUEST=1".into(),
                        "ANUBIS_ISOLATION=tart-disposable-guest".into(),
                        runner,
                        "fuzz".into(),
                        "/tmp/anubis-fuzz.anb".into(),
                        "--iterations".into(),
                        iterations.to_string(),
                        "--allow-research".into(),
                    ],
                )
            })
        }
    }
}

/// Derive + print the hypervisor confinement manifest from a program's proven effect set. Fails
/// closed: the program must PARSE and pass `anubis check` (there is no proof to confine from
/// otherwise). Never boots a VM. The manifest is the same one auto-sealed into every evidence bundle.
fn run_confine(program: &str, out: Option<String>) -> Result<()> {
    let src =
        std::fs::read_to_string(program).with_context(|| format!("read program `{program}`"))?;
    let ast = anubis_compiler::parse_source(&src)
        .map_err(|e| anyhow!("ANUBIS_CONFINE_PARSE_FAILED: {e}"))?;
    let mode = crate::first_mode(&ast.items).unwrap_or(anubis_compiler::frontend::Mode::Safe);
    // Fail closed: a program that does not typecheck has no PROVEN effect set to derive from.
    anubis_compiler::typecheck(ast, mode).map_err(|e| {
        anyhow!(
            "ANUBIS_CONFINE_UNVERIFIED: refusing to derive a confinement policy from a program that \
             does not pass `anubis check` — confinement is only meaningful as a consequence of a \
             passing check: {e}"
        )
    })?;
    let manifest =
        anubis_compiler::package::confinement::derive_confinement("program", "0.0.0", &src)
            .map_err(|e| anyhow!("{e}"))?;
    let json = serde_json::to_string_pretty(&manifest).map_err(|e| anyhow!("{e}"))?;

    eprintln!(
        "[anubis vz confine] hypervisor confinement derived from the program's PROVEN effect set:"
    );
    eprintln!("  effects_bounded : {}", manifest.effects_bounded);
    eprintln!(
        "  capabilities    : {}",
        if manifest.capabilities_present.is_empty() {
            "(none proven — maximally confinable)".to_string()
        } else {
            manifest.capabilities_present.join(", ")
        }
    );
    for g in &manifest.grants {
        // Show the two hypervisor-relevant dimensions (network, mount) always; other caps only when present.
        let relevant = g.hypervisor_grant.starts_with("network")
            || g.hypervisor_grant.starts_with("mount")
            || g.present;
        if !relevant {
            continue;
        }
        let mark = if g.tart_enforced {
            "tart-enforced"
        } else {
            "advisory/needs-human"
        };
        let args = if g.tart_args.is_empty() {
            String::new()
        } else {
            format!("  ({})", g.tart_args.join(" "))
        };
        eprintln!(
            "  {:<9} -> {:<30} [{}]{}",
            g.capability, g.hypervisor_grant, mark, args
        );
    }
    eprintln!(
        "  (sealed + re-derived-on-verify as confinement_manifest.json in every `anubis build --evidence` bundle)"
    );

    if let Some(path) = out {
        std::fs::write(&path, &json).with_context(|| format!("write `{path}`"))?;
        eprintln!("[anubis vz confine] wrote {path}");
    } else {
        println!("{json}");
    }
    Ok(())
}

/// Boot a VM headless in the background and poll for its IP (up to ~60s). Idempotent: if already
/// running, returns the current IP.
fn wait_for_ip(name: &str) -> Result<String> {
    // If it isn't running yet, kick it off headless in the background.
    if tart_capture(&[s("ip"), name.into()]).is_err() {
        let bin = tart_bin()?;
        Command::new(bin)
            .args(["run", name, "--no-graphics"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("failed to boot `{name}`"))?;
    }
    for _ in 0..30 {
        if let Ok(ip) = tart_capture(&[s("ip"), name.into()]) {
            if !ip.is_empty() {
                return Ok(ip);
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
    bail!("ANUBIS_VZ_NO_IP: `{name}` did not report an IP within ~60s")
}

/// rsync a host path into a `user@ip:/dest` target over SSH (host-key checks relaxed for ephemeral IPs).
fn rsync_into(from: &str, dst: &str) -> Result<()> {
    let key = vz_ssh_identity()?;
    let quoted_key = shell_single_quote(&key.to_string_lossy());
    let transport = format!(
        "ssh -i {quoted_key} -o IdentitiesOnly=yes -o StrictHostKeyChecking=no \
         -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR"
    );
    let status = Command::new("rsync")
        .args(["-az", "-e", &transport, from, dst])
        .status()
        .with_context(|| "failed to spawn rsync (is it installed?)")?;
    if status.success() {
        Ok(())
    } else {
        bail!("ANUBIS_VZ_SYNC_FAILED: rsync `{from}` -> `{dst}` exited non-zero")
    }
}

/// Run a command in a guest over SSH, streaming output; map non-zero to an error.
fn ssh_exec(user: String, ip: String, command: &[String]) -> Result<()> {
    let target = format!("{user}@{ip}");
    let status = Command::new("ssh")
        .args(ssh_common_args()?)
        .arg(&target)
        .args(command)
        .status()
        .with_context(|| "failed to spawn ssh")?;
    if status.success() {
        Ok(())
    } else {
        bail!(
            "ANUBIS_VZ_EXEC_FAILED: remote command exited with {}",
            status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "signal".into())
        )
    }
}

/// Copy the exact currently-running Anubis binary into the disposable guest. This avoids relying
/// on a golden image's shell PATH or stale tool build, and makes the guest result bind to the host
/// command the operator actually invoked.
fn sync_current_anubis(user: &str, ip: &str) -> Result<String> {
    let current = std::env::current_exe().context("resolve current anubis executable")?;
    let current = current
        .to_str()
        .ok_or_else(|| anyhow!("ANUBIS_VZ_RUNNER_PATH: current executable path is not UTF-8"))?;
    let remote = "/tmp/anubis-vz-runner";
    rsync_into(current, &format!("{user}@{ip}:{remote}"))?;
    ssh_exec(
        user.to_string(),
        ip.to_string(),
        &["chmod".into(), "+x".into(), remote.into()],
    )?;
    Ok(remote.into())
}

/// Canonical Tart guest key. An explicit override supports hermetic CI tests and operators with a
/// relocated key, but SSH-agent identities are never consulted (`IdentitiesOnly=yes`).
fn vz_ssh_identity() -> Result<PathBuf> {
    let path = if let Some(path) = std::env::var_os("ANUBIS_VZ_SSH_KEY") {
        PathBuf::from(path)
    } else {
        let home = std::env::var_os("HOME").ok_or_else(|| {
            anyhow!("ANUBIS_VZ_SSH_KEY_MISSING: HOME is unavailable; set ANUBIS_VZ_SSH_KEY")
        })?;
        PathBuf::from(home).join(".ssh/tart_anubis")
    };
    if !path.is_file() {
        bail!(
            "ANUBIS_VZ_SSH_KEY_MISSING: canonical Tart identity `{}` does not exist; create it or \
             set ANUBIS_VZ_SSH_KEY",
            path.display()
        );
    }
    Ok(path)
}

fn ssh_common_args() -> Result<Vec<OsString>> {
    let key = vz_ssh_identity()?;
    Ok(ssh_common_args_for_key(&key))
}

fn ssh_common_args_for_key(key: &Path) -> Vec<OsString> {
    vec![
        "-i".into(),
        key.as_os_str().to_os_string(),
        "-o".into(),
        "IdentitiesOnly=yes".into(),
        "-o".into(),
        "StrictHostKeyChecking=no".into(),
        "-o".into(),
        "UserKnownHostsFile=/dev/null".into(),
        "-o".into(),
        "LogLevel=ERROR".into(),
    ]
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// The disposable-guest pattern: clone an ephemeral CoW guest from `base`, boot it, run `body(name,
/// ip)`, then DELETE the guest (unless `keep`) — even if the body errors. The blast radius of whatever
/// ran inside is the throwaway VM, never the host. The clone name is derived from the base + pid so a
/// caller need not manage names.
fn disposable<F: FnOnce(&str, String) -> Result<()>>(
    base: &str,
    keep: bool,
    body: F,
) -> Result<()> {
    let name = format!("anubis-vz-ephemeral-{}", std::process::id());
    eprintln!("[anubis vz] cloning disposable guest `{name}` from `{base}` (APFS CoW)");
    tart_run(&[s("clone"), base.into(), name.clone()])?;
    let result = wait_for_ip(&name).and_then(|ip| body(&name, ip));
    // Always tear down (best-effort) unless the operator asked to keep it.
    if keep {
        eprintln!("[anubis vz] keeping `{name}` (pass no --keep to auto-discard). Delete: anubis vz delete {name} --force");
    } else {
        let _ = tart_run(&[s("stop"), name.clone()]);
        let _ = tart_run(&[s("delete"), name.clone()]);
        eprintln!("[anubis vz] discarded disposable guest `{name}`");
    }
    result
}

fn indent(text: &str) -> String {
    text.lines()
        .map(|l| format!("    {l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod ssh_transport_tests {
    use super::*;

    #[test]
    fn common_ssh_args_pin_identity_and_disable_agent_fanout() {
        let key = Path::new("/tmp/key with space");
        let args = ssh_common_args_for_key(key);
        assert_eq!(args[0], "-i");
        assert_eq!(args[1], key.as_os_str());
        assert!(args.windows(2).any(|w| w == ["-o", "IdentitiesOnly=yes"]));
        let quoted = shell_single_quote(&key.to_string_lossy());
        assert_eq!(quoted, "'/tmp/key with space'");
        let rendered = format!("ssh -i {quoted} -o IdentitiesOnly=yes");
        assert!(rendered.contains("-o IdentitiesOnly=yes"));
    }
}
