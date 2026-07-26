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
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
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
    /// Fuzz a **local binary** in a DISPOSABLE guest (clone → boot → sync target binary →
    /// `anubis fuzz --target … --runs …` inside → discard). Matches the host fuzz CLI shape
    /// (`--target` + `--runs`); `iterations` is an alias for `--runs` on the outer command.
    Fuzz {
        /// Host path to the binary under test (staged into the guest and passed as `--target`).
        target: String,
        /// Maps to inner `anubis fuzz --runs` (process-mutation iterations).
        #[arg(long, default_value_t = 1000)]
        iterations: u64,
        #[arg(long, default_value = "anubis-xcode")]
        base: String,
        #[arg(long)]
        keep: bool,
        /// Required acknowledgment (offensive surface); guest isolation uses VZ markers, not this flag
        /// on the inner `fuzz` CLI (host `anubis fuzz` has no `--allow-research`).
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
            // Research/crash-class remote commands must carry a guest-bound single-use capability.
            let remote_command = if command_requires_run_capability(&command) {
                let cmd_material = command.join("\0");
                let cmd_digest = hex::encode(Sha256::digest(cmd_material.as_bytes()));
                let (cap_key, program_digest) = stage_run_capability_to_guest(
                    &user,
                    &ip,
                    &name,
                    "anubis-xcode",
                    None,
                    Some(&cmd_digest),
                    &["process.spawn", "vm.execute"],
                )?;
                let env = run_cap_env_fragment(&name, &program_digest, &cap_key);
                let quoted: Vec<String> = command.iter().map(|c| shell_single_quote(c)).collect();
                let shell = format!("env {env} {}", quoted.join(" "));
                eprintln!(
                    "[anubis vz] ssh {target} -- (research-gated; guest-bound run capability) {}",
                    command.join(" ")
                );
                vec!["bash".into(), "-lc".into(), shell]
            } else {
                eprintln!("[anubis vz] ssh {target} -- {}", command.join(" "));
                command
            };
            let status = Command::new("ssh")
                .args(ssh_common_args()?)
                .arg(&target)
                .args(&remote_command)
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
                const REMOTE_POC: &str = "/tmp/anubis-poc.anb";
                sync_path_verified(&user, &ip, &poc, REMOTE_POC)?;
                // Gold PoC-kit oracle (path used by examples/security/poc_local_overflow.anb).
                // Without this, disposable guests only receive the .anb + runner and spawn fails.
                let gold = Path::new("poc_kit/bin/vuln_local");
                if gold.is_file() {
                    let gold_s = gold.to_str().ok_or_else(|| {
                        anyhow!("ANUBIS_VZ_SYNC_FAILED: gold vuln path is not UTF-8")
                    })?;
                    sync_path_verified(&user, &ip, gold_s, "poc_kit/bin/vuln_local")?;
                    ssh_exec(
                        user.clone(),
                        ip.clone(),
                        &["chmod".into(), "+x".into(), "poc_kit/bin/vuln_local".into()],
                    )?;
                }
                let runner = sync_current_anubis(&user, &ip)?;
                let (cap_key, program_digest) = stage_run_capability_to_guest(
                    &user,
                    &ip,
                    name,
                    &base,
                    Some(Path::new(&poc)),
                    None,
                    &["process.spawn", "vm.execute"],
                )?;
                eprintln!(
                    "[anubis vz] running `anubis run /tmp/anubis-poc.anb --allow-research` in disposable `{name}` \
                     with guest-bound run capability (single-use)"
                );
                // `$HOME` so relative `poc_kit/bin/vuln_local` resolves after stage.
                let env = run_cap_env_fragment(name, &program_digest, &cap_key);
                let remote = format!(
                    "cd \"$HOME\" && env {env} {runner} run /tmp/anubis-poc.anb --allow-research",
                    runner = shell_single_quote(&runner),
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
            let host_target = Path::new(&target);
            if !host_target.is_file() {
                bail!(
                    "ANUBIS_VZ_FUZZ_TARGET_MISSING: host target `{}` is not a file (expected a local binary for `anubis fuzz --target`)",
                    host_target.display()
                );
            }
            let remote_target = guest_fuzz_target_path(&target)?;
            disposable(&base, keep, |name, ip| {
                // Stage as a binary path (not a fake .anb): host CLI is `fuzz --target <binary> --runs N`.
                sync_path_verified(&user, &ip, &target, &remote_target)?;
                ssh_exec(
                    user.clone(),
                    ip.clone(),
                    &["chmod".into(), "+x".into(), remote_target.clone()],
                )?;
                let runner = sync_current_anubis(&user, &ip)?;
                let (cap_key, program_digest) = stage_run_capability_to_guest(
                    &user,
                    &ip,
                    name,
                    &base,
                    Some(host_target),
                    None,
                    &["process.spawn", "vm.execute"],
                )?;
                eprintln!(
                    "[anubis vz] fuzzing in disposable `{name}` (`anubis fuzz --target {remote_target} --runs {iterations}`) \
                     with guest-bound run capability"
                );
                let remote = fuzz_guest_shell_command(
                    &runner,
                    &remote_target,
                    iterations,
                    name,
                    &program_digest,
                    &cap_key,
                );
                ssh_exec(user, ip, &["bash".into(), "-lc".into(), remote])
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
    let mode = crate::program_mode(&ast.items).unwrap_or(anubis_compiler::frontend::Mode::Safe);
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
    let remote = remote_command_line(command);
    let status = Command::new("ssh")
        .args(ssh_common_args()?)
        .arg(&target)
        .arg(remote)
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

/// Capture stdout of a remote command (trimmed); non-zero exit is an error.
fn ssh_capture(user: &str, ip: &str, command: &[String]) -> Result<String> {
    let target = format!("{user}@{ip}");
    let remote = remote_command_line(command);
    let out = Command::new("ssh")
        .args(ssh_common_args()?)
        .arg(&target)
        .arg(remote)
        .output()
        .with_context(|| "failed to spawn ssh (capture)")?;
    if !out.status.success() {
        bail!(
            "ANUBIS_VZ_EXEC_FAILED: remote capture exited with {}: {}",
            out.status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "signal".into()),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// SHA-256 hex of a local file (host).
fn file_sha256_hex(path: &Path) -> Result<String> {
    let bytes =
        std::fs::read(path).with_context(|| format!("read `{}` for SHA-256", path.display()))?;
    Ok(hex::encode(Sha256::digest(&bytes)))
}

/// Parse `shasum -a 256` / `sha256sum` first-field hex (64 lowercase hex chars).
fn parse_shasum_line(line: &str) -> Result<String> {
    let hex = line
        .split_whitespace()
        .next()
        .ok_or_else(|| anyhow!("ANUBIS_VZ_SHA256_PARSE: empty shasum output"))?;
    let hex = hex.to_ascii_lowercase();
    if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("ANUBIS_VZ_SHA256_PARSE: not a 64-char hex digest: {hex:?}");
    }
    Ok(hex)
}

/// Guest-side SHA-256 via `shasum -a 256` (macOS goldens) with `sha256sum` fallback.
fn remote_sha256_hex(user: &str, ip: &str, remote_path: &str) -> Result<String> {
    let quoted = shell_single_quote(remote_path);
    let script = format!(
        "if command -v shasum >/dev/null 2>&1; then shasum -a 256 {quoted}; \
         elif command -v sha256sum >/dev/null 2>&1; then sha256sum {quoted}; \
         else echo 'ANUBIS_VZ_SHA256_TOOL_MISSING' >&2; exit 2; fi"
    );
    let out = ssh_capture(user, ip, &["bash".into(), "-lc".into(), script])?;
    parse_shasum_line(&out)
}

/// Rsync host path → guest path, then fail-closed if guest digest ≠ host digest.
fn sync_path_verified(user: &str, ip: &str, host_path: &str, remote_path: &str) -> Result<()> {
    let host = Path::new(host_path);
    if !host.is_file() {
        bail!(
            "ANUBIS_VZ_SYNC_FAILED: host path `{}` is not a file",
            host.display()
        );
    }
    let expect = file_sha256_hex(host)?;
    // Ensure remote parent exists for nested stage paths.
    if let Some(parent) = Path::new(remote_path).parent() {
        let p = parent.to_string_lossy();
        if p != "/" && !p.is_empty() {
            ssh_exec(
                user.to_string(),
                ip.to_string(),
                &["mkdir".into(), "-p".into(), p.into_owned()],
            )?;
        }
    }
    rsync_into(host_path, &format!("{user}@{ip}:{remote_path}"))?;
    let got = remote_sha256_hex(user, ip, remote_path)?;
    if got != expect {
        bail!(
            "ANUBIS_VZ_SHA256_MISMATCH: host `{}` sha256={expect} guest `{remote_path}` sha256={got} \
             — refusing to execute (tamper or incomplete sync)",
            host.display()
        );
    }
    Ok(())
}

/// Guest path for a staged fuzz **binary** (basename under `/tmp/anubis-fuzz/`).
fn guest_fuzz_target_path(host_target: &str) -> Result<String> {
    let name = Path::new(host_target)
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| {
            anyhow!("ANUBIS_VZ_FUZZ_TARGET: cannot derive basename from `{host_target}`")
        })?;
    if name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\\') {
        bail!("ANUBIS_VZ_FUZZ_TARGET: unsafe basename `{name}`");
    }
    Ok(format!("/tmp/anubis-fuzz/{name}"))
}

/// Inner guest shell for process fuzz — must match host `anubis fuzz --target … --runs …`
/// (not positional .anb, not `--iterations`, not `--allow-research` on the fuzz subcommand).
fn fuzz_guest_shell_command(
    runner: &str,
    remote_target: &str,
    runs: u64,
    guest_id: &str,
    program_digest: &str,
    cap_key: &str,
) -> String {
    format!(
        "env ANUBIS_VZ_GUEST=1 ANUBIS_OFFENSIVE_GATE_IN_GUEST=1 \
         ANUBIS_ISOLATION=tart-disposable-guest \
         ANUBIS_VZ_ENFORCE_RUN_CAP=1 \
         ANUBIS_RUN_CAP_PATH=/tmp/anubis-run-cap.json \
         ANUBIS_RUN_CAP_KEY={key} \
         ANUBIS_VZ_GUEST_ID={gid} \
         ANUBIS_PROGRAM_DIGEST={pd} \
         ANUBIS_ENGAGEMENT_ID=vz-session \
         ANUBIS_ENGAGEMENT_HASH=vz-session-hash \
         {runner} fuzz --target {target} --runs {runs}",
        runner = shell_single_quote(runner),
        target = shell_single_quote(remote_target),
        runs = runs,
        key = shell_single_quote(cap_key),
        gid = shell_single_quote(guest_id),
        pd = shell_single_quote(program_digest),
    )
}

const REMOTE_RUN_CAP: &str = "/tmp/anubis-run-cap.json";

/// True when a remote command is crash/research/offensive-class and must carry a guest-bound cap.
fn command_requires_run_capability(command: &[String]) -> bool {
    let joined = command.join(" ").to_ascii_lowercase();
    joined.contains("--allow-research")
        || joined.contains(" allow-research")
        || joined.contains("anubis fuzz")
        || joined.contains(" fuzz ")
        || joined.contains("exploit")
        || joined.contains("target_run")
        || joined.contains("agent-generate")
        || joined.contains(" listen")
        || joined.ends_with("listen")
        || joined.contains("task-queue")
}

/// Resolve allowed effects for a guest run capability from Anubis source when available.
///
/// Uses the shared `ProvenEffectSet` IR (same fixpoint as `anubis vz confine`). Falls back to
/// the caller-supplied defaults (or research defaults) when source is missing / not Anubis.
fn resolve_run_cap_effects(source_path: Option<&Path>, fallback: &[&str]) -> Vec<String> {
    if let Some(p) = source_path {
        let is_anb = p
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("anb"))
            .unwrap_or(false);
        if is_anb {
            if let Ok(src) = std::fs::read_to_string(p) {
                // Prefer typecheck path (TypedIR.proven_effects); fall back to parse-only fixpoint.
                let proven = anubis_compiler::research_profile::proven_effects_via_typecheck(&src)
                    .or_else(|e| {
                        eprintln!(
                            "[anubis vz] typecheck proven-effects unavailable ({e}); using parse fixpoint"
                        );
                        anubis_compiler::research_profile::proven_effects_from_source(&src)
                    });
                match proven {
                    Ok(proven) => {
                        let mut names = proven.for_run_capability();
                        // Merge any explicit fallback extras (e.g. debug.attach) the caller passed.
                        for e in fallback {
                            if !names.iter().any(|n| n == *e) {
                                names.push((*e).to_string());
                            }
                        }
                        if !names.iter().any(|n| n == "vm.execute") {
                            names.push("vm.execute".into());
                        }
                        // Never mint with an empty effect list (fail-open guard).
                        if names.is_empty() {
                            names = anubis_compiler::research_profile::ProvenEffectSet::default_research_run_effects();
                        }
                        return names;
                    }
                    Err(e) => {
                        eprintln!(
                            "[anubis vz] WARN: proven-effects from source failed ({e}); using fallback effect list"
                        );
                    }
                }
            }
        }
    }
    if fallback.is_empty() {
        anubis_compiler::research_profile::ProvenEffectSet::default_research_run_effects()
    } else {
        let mut allowed: Vec<String> = fallback.iter().map(|e| (*e).to_string()).collect();
        if !allowed.iter().any(|e| e == "vm.execute") {
            allowed.push("vm.execute".into());
        }
        allowed
    }
}

/// Mint a single-use guest-bound capability JSON on the host.
///
/// `source_path` when set is hashed as both source and program digest; otherwise
/// `program_digest` must be supplied (e.g. sha of remote command string).
/// When `source_path` is an `.anb` file, allowed effects are derived from the shared
/// proven-effect IR (aligned with `vz confine`); otherwise `effects` is used as fallback.
fn mint_and_write_guest_cap(
    guest_id: &str,
    base: &str,
    source_path: Option<&Path>,
    program_digest: Option<&str>,
    effects: &[&str],
) -> Result<(PathBuf, String, String)> {
    use rand::RngCore;
    let mut key_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut key_bytes);
    let key = hex::encode(key_bytes);
    let source_digest = if let Some(p) = source_path {
        file_sha256_hex(p)?
    } else {
        program_digest.unwrap_or("vz-no-source").to_string()
    };
    let program_digest = program_digest
        .map(|s| s.to_string())
        .unwrap_or_else(|| source_digest.clone());
    let compiler = std::env::current_exe().context("resolve anubis for capability digests")?;
    let compiler_digest = file_sha256_hex(&compiler)?;
    let allowed = resolve_run_cap_effects(source_path, effects);
    let cap =
        crate::offensive::run_capability::mint(crate::offensive::run_capability::MintParams {
            key: &key,
            engagement_id: "vz-session",
            engagement_hash: "vz-session-hash",
            authorization_digest: "vz-orchestrator-auth",
            source_digest: &source_digest,
            compiler_digest: &compiler_digest,
            program_digest: &program_digest,
            guest_id,
            base_digest: base,
            confinement_digest: "tart-disposable",
            allowed_effects: allowed,
            allowed_targets: vec![],
            operator: "vz-operator",
            ttl_secs: 3600,
        });
    let dir = std::env::temp_dir().join(format!("anubis-run-cap-{}", guest_id));
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("run_capability.json");
    crate::offensive::run_capability::write_cap(&path, &cap)?;
    Ok((path, key, program_digest))
}

/// Mint + rsync capability into guest; returns (cap_key, program_digest) for env injection.
fn stage_run_capability_to_guest(
    user: &str,
    ip: &str,
    guest_id: &str,
    base: &str,
    source_path: Option<&Path>,
    program_digest: Option<&str>,
    effects: &[&str],
) -> Result<(String, String)> {
    let (cap_path, cap_key, program_digest) =
        mint_and_write_guest_cap(guest_id, base, source_path, program_digest, effects)?;
    let cap_host = cap_path
        .to_str()
        .ok_or_else(|| anyhow!("ANUBIS_RUN_CAP_PATH: non-UTF8"))?;
    sync_path_verified(user, ip, cap_host, REMOTE_RUN_CAP)?;
    Ok((cap_key, program_digest))
}

/// Shell env fragment that enforces guest-bound run capability validation.
fn run_cap_env_fragment(guest_id: &str, program_digest: &str, cap_key: &str) -> String {
    format!(
        "ANUBIS_VZ_GUEST=1 ANUBIS_OFFENSIVE_GATE_IN_GUEST=1 \
         ANUBIS_ISOLATION=tart-disposable-guest \
         ANUBIS_VZ_ENFORCE_RUN_CAP=1 \
         ANUBIS_RUN_CAP_PATH={cap} \
         ANUBIS_RUN_CAP_KEY={key} \
         ANUBIS_VZ_GUEST_ID={gid} \
         ANUBIS_PROGRAM_DIGEST={pd} \
         ANUBIS_ENGAGEMENT_ID=vz-session \
         ANUBIS_ENGAGEMENT_HASH=vz-session-hash",
        cap = shell_single_quote(REMOTE_RUN_CAP),
        key = shell_single_quote(cap_key),
        gid = shell_single_quote(guest_id),
        pd = shell_single_quote(program_digest),
    )
}

/// Copy the exact currently-running Anubis binary into the disposable guest. This avoids relying
/// on a golden image's shell PATH or stale tool build, and makes the guest result bind to the host
/// command the operator actually invoked. Digest is verified host→guest before return.
fn sync_current_anubis(user: &str, ip: &str) -> Result<String> {
    let current = std::env::current_exe().context("resolve current anubis executable")?;
    let current = current
        .to_str()
        .ok_or_else(|| anyhow!("ANUBIS_VZ_RUNNER_PATH: current executable path is not UTF-8"))?;
    let remote = "/tmp/anubis-vz-runner";
    sync_path_verified(user, ip, current, remote)?;
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

fn remote_command_line(command: &[String]) -> String {
    command
        .iter()
        .map(|part| shell_single_quote(part))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Combine body + stop/delete outcomes. Teardown failure is never silently labeled "discarded".
fn finalize_disposable_teardown(
    name: &str,
    body: Result<()>,
    stop: Result<()>,
    delete: Result<()>,
) -> Result<()> {
    match (&stop, &delete) {
        (Ok(()), Ok(())) => {
            eprintln!("[anubis vz] discarded disposable guest `{name}`");
            body
        }
        _ => {
            let msg = format_teardown_failure(name, &stop, &delete);
            match body {
                Ok(()) => bail!("{msg}"),
                Err(e) => bail!("{msg}; body also failed: {e:#}"),
            }
        }
    }
}

fn format_teardown_failure(name: &str, stop: &Result<()>, delete: &Result<()>) -> String {
    let mut msg = format!("ANUBIS_VZ_TEARDOWN_FAILED: guest `{name}` was not fully discarded");
    if let Err(e) = stop {
        msg.push_str(&format!("; stop: {e:#}"));
    }
    if let Err(e) = delete {
        msg.push_str(&format!("; delete: {e:#}"));
    }
    msg
}

/// The disposable-guest pattern: clone an ephemeral CoW guest from `base`, boot it, run `body(name,
/// ip)`, then DELETE the guest (unless `keep`) — even if the body errors. The blast radius of whatever
/// ran inside is the throwaway VM, never the host. The clone name is derived from the base + pid so a
/// caller need not manage names.
///
/// Teardown is **fail-closed**: if `tart stop` or `tart delete` fails, this returns
/// `ANUBIS_VZ_TEARDOWN_FAILED` and does **not** print a false "discarded" success line.
fn disposable<F: FnOnce(&str, String) -> Result<()>>(
    base: &str,
    keep: bool,
    body: F,
) -> Result<()> {
    let name = format!("anubis-vz-ephemeral-{}", std::process::id());
    eprintln!("[anubis vz] cloning disposable guest `{name}` from `{base}` (APFS CoW)");
    tart_run(&[s("clone"), base.into(), name.clone()])?;
    let result = wait_for_ip(&name).and_then(|ip| body(&name, ip));
    if keep {
        eprintln!("[anubis vz] keeping `{name}` (pass no --keep to auto-discard). Delete: anubis vz delete {name} --force");
        return result;
    }
    let stop = tart_run(&[s("stop"), name.clone()]);
    let delete = tart_run(&[s("delete"), name.clone()]);
    finalize_disposable_teardown(&name, result, stop, delete)
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

    #[test]
    fn fuzz_guest_command_matches_host_fuzz_cli_shape() {
        // Host: `anubis fuzz --target <bin> --runs N` — NOT positional .anb / --iterations / --allow-research.
        let cmd = fuzz_guest_shell_command(
            "/tmp/anubis-vz-runner",
            "/tmp/anubis-fuzz/vuln_local",
            42,
            "guest-test-1",
            "deadbeef",
            "cap-key-hex",
        );
        assert!(
            cmd.contains("fuzz --target '/tmp/anubis-fuzz/vuln_local' --runs 42"),
            "unexpected fuzz guest cmd: {cmd}"
        );
        assert!(
            !cmd.contains("--iterations"),
            "must not pass --iterations to inner fuzz: {cmd}"
        );
        assert!(
            !cmd.contains("fuzz --allow-research") && !cmd.contains("fuzz '/tmp"),
            "must not use --allow-research or positional path on fuzz: {cmd}"
        );
        assert!(cmd.contains("ANUBIS_VZ_GUEST=1"));
        assert!(cmd.contains("ANUBIS_ISOLATION=tart-disposable-guest"));
        assert!(cmd.contains("ANUBIS_VZ_ENFORCE_RUN_CAP=1"));
        assert!(cmd.contains("ANUBIS_RUN_CAP_PATH=/tmp/anubis-run-cap.json"));
        assert!(!cmd.contains(".anb"));
    }

    #[test]
    fn command_requires_run_capability_classifies_research_paths() {
        assert!(command_requires_run_capability(&[
            "anubis".into(),
            "run".into(),
            "poc.anb".into(),
            "--allow-research".into(),
        ]));
        assert!(command_requires_run_capability(&[
            "anubis".into(),
            "fuzz".into(),
            "--target".into(),
            "bin".into(),
        ]));
        assert!(command_requires_run_capability(&[
            "anubis".into(),
            "agent-generate".into(),
        ]));
        assert!(command_requires_run_capability(&[
            "anubis".into(),
            "listen".into(),
            "--bind".into(),
            "127.0.0.1:0".into(),
        ]));
        assert!(!command_requires_run_capability(&[
            "echo".into(),
            "hello".into(),
        ]));
        assert!(!command_requires_run_capability(&[
            "anubis".into(),
            "check".into(),
            "safe.anb".into(),
        ]));
    }

    #[test]
    fn run_cap_env_fragment_enforces_guest_bound_fields() {
        let env = run_cap_env_fragment("guest-1", "digest-aa", "key-bb");
        assert!(env.contains("ANUBIS_VZ_ENFORCE_RUN_CAP=1"));
        assert!(env.contains("ANUBIS_RUN_CAP_PATH='/tmp/anubis-run-cap.json'"));
        assert!(env.contains("ANUBIS_RUN_CAP_KEY='key-bb'"));
        assert!(env.contains("ANUBIS_VZ_GUEST_ID='guest-1'"));
        assert!(env.contains("ANUBIS_PROGRAM_DIGEST='digest-aa'"));
        assert!(env.contains("ANUBIS_ENGAGEMENT_ID=vz-session"));
        assert!(env.contains("ANUBIS_VZ_GUEST=1"));
    }

    #[test]
    fn mint_and_write_guest_cap_produces_readable_json() {
        let dir = tempfile::tempdir().unwrap();
        // Use a real empty file as source so digest is stable.
        let src = dir.path().join("poc.anb");
        std::fs::write(&src, b"// lab").unwrap();
        let (path, key, pd) = mint_and_write_guest_cap(
            "unit-guest",
            "anubis-xcode",
            Some(&src),
            None,
            &["process.spawn", "vm.execute"],
        )
        .expect("mint");
        assert!(!key.is_empty());
        assert_eq!(pd.len(), 64); // sha256 hex
        let cap = crate::offensive::run_capability::read_cap(&path).expect("read cap");
        assert_eq!(cap.guest_id, "unit-guest");
        assert!(cap.allowed_effects.iter().any(|e| e == "vm.execute"));
        assert!(cap.allowed_effects.iter().any(|e| e == "process.spawn"));
        assert_eq!(cap.program_digest, pd);
    }

    #[test]
    fn mint_from_anb_source_uses_proven_effect_ir() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("beacon.anb");
        std::fs::write(
            &src,
            "fn beacon() uses(net.send) { http_post(\"http://x/y\", \"z\"); }\n\
             fn main() uses(net.send) { beacon(); }\n",
        )
        .unwrap();
        let (path, _key, _pd) = mint_and_write_guest_cap(
            "unit-guest-net",
            "anubis-xcode",
            Some(&src),
            None,
            &["process.spawn"], // fallback ignored when .anb proves effects
        )
        .expect("mint");
        let cap = crate::offensive::run_capability::read_cap(&path).expect("read cap");
        assert!(
            cap.allowed_effects.iter().any(|e| e == "net.connect"),
            "proven net.send must become net.connect in run cap: {:?}",
            cap.allowed_effects
        );
        assert!(cap.allowed_effects.iter().any(|e| e == "vm.execute"));
        // Must not only list legacy net.send without research IR
        assert!(
            !cap.allowed_effects
                .iter()
                .all(|e| e == "process.spawn" || e == "vm.execute"),
            "must include proven net effect, not only fallback: {:?}",
            cap.allowed_effects
        );
    }

    #[test]
    fn resolve_run_cap_effects_binary_fallback() {
        let bin = Path::new("/tmp/vuln_local");
        let effects = resolve_run_cap_effects(Some(bin), &["process.spawn"]);
        assert!(effects.iter().any(|e| e == "vm.execute"));
        assert!(effects.iter().any(|e| e == "process.spawn"));
        assert!(!effects.iter().any(|e| e == "net.connect"));
    }

    #[test]
    fn guest_fuzz_target_path_uses_basename_under_stage_dir() {
        assert_eq!(
            guest_fuzz_target_path("poc_kit/bin/vuln_local").unwrap(),
            "/tmp/anubis-fuzz/vuln_local"
        );
        assert_eq!(
            guest_fuzz_target_path("/abs/path/mybin").unwrap(),
            "/tmp/anubis-fuzz/mybin"
        );
        assert!(guest_fuzz_target_path("").is_err());
    }

    #[test]
    fn parse_shasum_line_accepts_mac_and_gnu_shapes() {
        assert_eq!(
            parse_shasum_line(
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  -"
            )
            .unwrap(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            parse_shasum_line(
                "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855  /tmp/x"
            )
            .unwrap(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert!(parse_shasum_line("not-a-hash").is_err());
        assert!(parse_shasum_line("").is_err());
    }

    #[test]
    fn remote_command_line_preserves_bash_lc_payload_as_one_arg() {
        let line = remote_command_line(&[
            "bash".into(),
            "-lc".into(),
            "if command -v shasum >/dev/null 2>&1; then shasum -a 256 '/tmp/x'; fi".into(),
        ]);
        assert_eq!(
            line,
            "'bash' '-lc' 'if command -v shasum >/dev/null 2>&1; then shasum -a 256 '\\''/tmp/x'\\''; fi'"
        );
    }

    #[test]
    fn file_sha256_hex_empty_file_is_known_digest() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("empty.bin");
        std::fs::write(&p, b"").unwrap();
        assert_eq!(
            file_sha256_hex(&p).unwrap(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn teardown_success_preserves_body_error_and_does_not_claim_discard_on_failure() {
        let body_err = Err(anyhow!("body boom"));
        let out = finalize_disposable_teardown("anubis-vz-ephemeral-1", body_err, Ok(()), Ok(()));
        assert!(out.is_err());
        let s = format!("{:#}", out.unwrap_err());
        assert!(s.contains("body boom"), "{s}");

        let fail_stop =
            finalize_disposable_teardown("g1", Ok(()), Err(anyhow!("stop failed")), Ok(()));
        let s = format!("{:#}", fail_stop.unwrap_err());
        assert!(s.contains("ANUBIS_VZ_TEARDOWN_FAILED"), "{s}");
        assert!(s.contains("stop failed"), "{s}");
        // Success line is "discarded disposable guest" — must not appear on teardown failure.
        assert!(
            !s.contains("discarded disposable guest"),
            "must not claim discard success: {s}"
        );

        let both = finalize_disposable_teardown(
            "g2",
            Err(anyhow!("payload")),
            Err(anyhow!("stop x")),
            Err(anyhow!("delete y")),
        );
        let s = format!("{:#}", both.unwrap_err());
        assert!(s.contains("ANUBIS_VZ_TEARDOWN_FAILED"), "{s}");
        assert!(s.contains("body also failed"), "{s}");
        assert!(s.contains("payload"), "{s}");
    }

    #[test]
    fn format_teardown_failure_names_guest_and_ops() {
        let msg = format_teardown_failure(
            "ephem-9",
            &Err(anyhow!("stop no")),
            &Err(anyhow!("delete no")),
        );
        assert!(msg.contains("ephem-9"));
        assert!(msg.contains("ANUBIS_VZ_TEARDOWN_FAILED"));
        assert!(msg.contains("stop no") && msg.contains("delete no"));
    }
}
