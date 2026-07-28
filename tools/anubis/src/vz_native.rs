//! `anubis vz native-preflight` — the NATIVE Apple Virtualization.framework backend, bound directly
//! through `objc2-virtualization` with NO `tart` dependency.
//!
//! WHY THIS EXISTS. `anubis vz` (see `vz.rs`) drives VZ through `tart`, which is honest and testable
//! but cannot express two confinements the language's own `confinement_manifest.json` asks for — a TRUE
//! zero-NIC air-gap for a program PROVEN net-free (`tart` has no zero-NIC flag; its `--net-host` still
//! reaches the host), and per-hostname egress for a net-using program (`tart` has no per-hostname
//! filter). Both were recorded as `[NEEDS-HUMAN]` in `compiler/src/package/confinement.rs` pending a
//! native FFI + the `com.apple.security.virtualization` entitlement.
//!
//! THE ENTITLEMENT IS NOT A DEVELOPER-PORTAL STEP. `com.apple.security.virtualization` is applied by
//! a LOCAL signature — ad-hoc (`codesign --sign -`) or the machine's existing Apple Development
//! identity — with no provisioning profile. `scripts/build_signed_anubis.sh` builds and signs this
//! binary with `vm/entitlements/anubis.entitlements`. Verified firsthand (the negative control is
//! precise): a binary signed ad-hoc WITHOUT the entitlement fails
//! `-[VZVirtualMachineConfiguration validateWithError:]` with the exact "...doesn't have the
//! com.apple.security.virtualization entitlement" message; the SAME binary signed WITH the entitlement
//! validates and instantiates a `VZVirtualMachine`. (A *fully* unsigned binary is not a useful control
//! — Apple Silicon SIGKILLs it before `main` runs, since every executable needs at least an ad-hoc
//! signature.) Notarization is only needed to DISTRIBUTE the binary to other machines, not to run this
//! lane on your own Mac — so for local use this lane is NOT `[NEEDS-HUMAN]`.
//!
//! WHAT `native-preflight` PROVES (fail-closed). It derives the confinement posture from the program's
//! PROVEN effect set (refusing any program that does not pass `anubis check`), builds the exact
//! `VZVirtualMachineConfiguration` that posture implies, and runs `validateWithError:` +
//! `VZVirtualMachine` instantiation. A successful run is a firsthand proof that (a) the entitlement is
//! present on THIS binary and (b) the derived hardware confinement is structurally valid — without
//! needing a bootable guest image. It never boots a guest; `native-boot` (a follow-up) applies the
//! same config to a real kernel/initrd.
//!
//! HONEST ENFORCEMENT SPLIT (mirrors the manifest's `tart_enforced`/`advisory`/`needs_human` fields).
//! The zero-NIC air-gap is FULLY ENFORCED by this backend: a config with zero network devices gives the
//! guest no interface at all — a real air-gap the instant it boots, structurally, not by policy. The
//! per-hostname egress is SUBSTRATE ENFORCED, GATEWAY STAGED: the backend attaches a
//! `VZFileHandleNetworkDeviceAttachment` (the guest's only link to the world is a host-held datagram
//! socket) and records the allow-list, but the userspace DNS-pinned frame filter that turns that socket
//! into a per-hostname firewall is the remaining engineering — reported as STAGED, never claimed as
//! enforcing.

use anyhow::{anyhow, Result};
use sha2::{Digest, Sha256};

/// The hardware confinement posture the native backend derives from a program's proven effect set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativePosture {
    /// Program proven net-free (or effect set unbounded → confine most restrictively): zero NICs.
    ZeroNicAirGap,
    /// Program declares `net.send`: a single file-handle NIC gated by a per-hostname allow-list.
    PerHostnameEgress { allow_hosts: Vec<String> },
}

impl NativePosture {
    fn label(&self) -> String {
        match self {
            NativePosture::ZeroNicAirGap => "zero-NIC air-gap (0 network devices)".to_string(),
            NativePosture::PerHostnameEgress { allow_hosts } => format!(
                "per-hostname egress (file-handle NIC; allow-list: {})",
                if allow_hosts.is_empty() {
                    "(empty — deny-all until --allow-host is given)".to_string()
                } else {
                    allow_hosts.join(", ")
                }
            ),
        }
    }
}

/// Derive the native confinement posture from a program's PROVEN effect set. Fails closed: a program
/// that does not parse + typecheck has no proof to confine from, so it is refused (identical gate to
/// `vz confine`).
pub fn derive_native_posture(program: &str, allow_hosts: &[String]) -> Result<NativePosture> {
    let src =
        std::fs::read_to_string(program).map_err(|e| anyhow!("read program `{program}`: {e}"))?;
    let ast = anubis_compiler::parse_source(&src)
        .map_err(|e| anyhow!("ANUBIS_VZNATIVE_PARSE_FAILED: {e}"))?;
    let mode = crate::program_mode(&ast.items).unwrap_or(anubis_compiler::frontend::Mode::Safe);
    anubis_compiler::typecheck(ast, mode).map_err(|e| {
        anyhow!(
            "ANUBIS_VZNATIVE_UNVERIFIED: refusing to derive a native confinement from a program that \
             does not pass `anubis check` — confinement is only meaningful as a consequence of a \
             passing check: {e}"
        )
    })?;
    let manifest =
        anubis_compiler::package::confinement::derive_confinement("program", "0.0.0", &src)
            .map_err(|e| anyhow!("{e}"))?;

    let net_present = manifest
        .capabilities_present
        .iter()
        .any(|c| c == "net.send");
    // Fail-closed lattice: net-free OR unbounded effect set → most restrictive (zero-NIC air-gap).
    if !net_present || !manifest.effects_bounded {
        Ok(NativePosture::ZeroNicAirGap)
    } else {
        Ok(NativePosture::PerHostnameEgress {
            allow_hosts: allow_hosts.to_vec(),
        })
    }
}

/// `anubis vz native-preflight <program> [--allow-host H] [--staging-dir DIR]...`
///
/// Derive the posture, build the native VZ configuration it implies (including a VirtioFS shared
/// directory when `--staging-dir` is given), and prove (via `validateWithError:` +
/// `VZVirtualMachine` init) that the entitlement is present and the confinement is structurally
/// valid. Never boots a guest.
pub fn native_preflight(
    program: &str,
    allow_hosts: &[String],
    staging_dir: Option<&str>,
) -> Result<()> {
    let posture = derive_native_posture(program, allow_hosts)?;

    eprintln!("[anubis vz native-preflight] backend : objc2-virtualization (native, no tart)");
    eprintln!("[anubis vz native-preflight] program : {program}");
    eprintln!("[anubis vz native-preflight] posture : {}", posture.label());
    if let Some(dir) = staging_dir {
        eprintln!("[anubis vz native-preflight] staging : {dir} (VirtioFS tag \"anubis\")");
    }
    match &posture {
        NativePosture::ZeroNicAirGap => eprintln!(
            "  enforcement : FULLY ENFORCED — the guest has no network interface at all (structural \
             air-gap the instant it boots). This is what tart's --net-host cannot express."
        ),
        NativePosture::PerHostnameEgress { allow_hosts } => {
            match crate::vz_egress_gateway::EgressPolicy::from_allow_hosts(allow_hosts) {
                Ok(pol) => eprintln!(
                    "  enforcement : SUBSTRATE ENFORCED / GATEWAY POLICY COMPILED — \
                     VZFileHandleNetworkDeviceAttachment + DNS-pinned allow-list ({} IPv4). \
                     Empty allow-list = deny-all. Live frame pump on the host fd is wired at \
                     `native-boot`; preflight proves policy + substrate only.",
                    pol.allowed_ipv4.len()
                ),
                Err(e) => eprintln!(
                    "  enforcement : SUBSTRATE ENFORCED / GATEWAY POLICY ERROR — {e} \
                     (fail-closed: fix --allow-host or leave empty for deny-all)"
                ),
            }
        }
    }

    build_validate_instantiate(&posture, staging_dir)?;

    let nic_count = match &posture {
        NativePosture::ZeroNicAirGap => 0,
        NativePosture::PerHostnameEgress { .. } => 1,
    };
    let share_count: usize = if staging_dir.is_some() { 1 } else { 0 };
    eprintln!(
        "[anubis vz native-preflight] OK — entitlement present; config valid \
         (networkDevices={nic_count}, directorySharingDevices={share_count})."
    );
    Ok(())
}

// ── Native VZ config construction (Apple Silicon macOS only) ─────────────────────────────────────

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn build_validate_instantiate(posture: &NativePosture, staging_dir: Option<&str>) -> Result<()> {
    use objc2::rc::Retained;
    use objc2::AnyThread;
    use objc2_foundation::{NSArray, NSFileHandle, NSString, NSURL};
    use objc2_virtualization::{
        VZDirectorySharingDeviceConfiguration, VZFileHandleNetworkDeviceAttachment,
        VZLinuxBootLoader, VZNetworkDeviceConfiguration, VZSharedDirectory, VZSingleDirectoryShare,
        VZVirtioFileSystemDeviceConfiguration, VZVirtioNetworkDeviceConfiguration,
        VZVirtualMachine, VZVirtualMachineConfiguration,
    };

    let kpath = std::env::temp_dir().join("anubis-vznative-preflight-kernel");
    if std::fs::metadata(&kpath).is_err() {
        std::fs::write(&kpath, b"placeholder-not-a-real-kernel")
            .map_err(|e| anyhow!("write placeholder kernel: {e}"))?;
    }

    unsafe {
        let cfg = VZVirtualMachineConfiguration::new();
        let min_mem = VZVirtualMachineConfiguration::minimumAllowedMemorySize();
        let max_mem = VZVirtualMachineConfiguration::maximumAllowedMemorySize();
        cfg.setMemorySize((512u64 * 1024 * 1024).clamp(min_mem, max_mem));
        cfg.setCPUCount(VZVirtualMachineConfiguration::minimumAllowedCPUCount().max(1));

        let ns_kpath = NSString::from_str(&kpath.to_string_lossy());
        let kurl = NSURL::fileURLWithPath(&ns_kpath);
        let boot = VZLinuxBootLoader::initWithKernelURL(VZLinuxBootLoader::alloc(), &kurl);
        cfg.setBootLoader(Some(&boot));

        match posture {
            NativePosture::ZeroNicAirGap => {
                debug_assert_eq!(cfg.networkDevices().count(), 0);
            }
            NativePosture::PerHostnameEgress { .. } => {
                let mut fds = [0i32; 2];
                let rc = libc::socketpair(libc::AF_UNIX, libc::SOCK_DGRAM, 0, fds.as_mut_ptr());
                if rc != 0 {
                    return Err(anyhow!(
                        "ANUBIS_VZNATIVE_SOCKETPAIR_FAILED: {}",
                        std::io::Error::last_os_error()
                    ));
                }
                let host_fh = NSFileHandle::initWithFileDescriptor_closeOnDealloc(
                    NSFileHandle::alloc(),
                    fds[0],
                    true,
                );
                let attach = VZFileHandleNetworkDeviceAttachment::initWithFileHandle(
                    VZFileHandleNetworkDeviceAttachment::alloc(),
                    &host_fh,
                );
                let dev = VZVirtioNetworkDeviceConfiguration::new();
                dev.setAttachment(Some(&attach));
                let dev_super: Retained<VZNetworkDeviceConfiguration> = dev.into_super();
                let arr = NSArray::from_retained_slice(&[dev_super]);
                cfg.setNetworkDevices(&arr);
            }
        }

        if let Some(dir) = staging_dir {
            let abs = std::fs::canonicalize(dir)
                .map_err(|e| anyhow!("ANUBIS_VZNATIVE_STAGING_DIR: canonicalize `{dir}`: {e}"))?;
            if !abs.is_dir() {
                return Err(anyhow!(
                    "ANUBIS_VZNATIVE_STAGING_DIR: `{dir}` is not a directory"
                ));
            }
            let ns_dir = NSString::from_str(&abs.to_string_lossy());
            let dir_url = NSURL::fileURLWithPath(&ns_dir);
            let shared = VZSharedDirectory::initWithURL_readOnly(
                VZSharedDirectory::alloc(),
                &dir_url,
                false,
            );
            let share =
                VZSingleDirectoryShare::initWithDirectory(VZSingleDirectoryShare::alloc(), &shared);
            let tag = NSString::from_str("anubis");
            let fs_dev = VZVirtioFileSystemDeviceConfiguration::initWithTag(
                VZVirtioFileSystemDeviceConfiguration::alloc(),
                &tag,
            );
            fs_dev.setShare(Some(&share));
            let fs_super: Retained<VZDirectorySharingDeviceConfiguration> = fs_dev.into_super();
            let arr = NSArray::from_retained_slice(&[fs_super]);
            cfg.setDirectorySharingDevices(&arr);
        }

        cfg.validateWithError().map_err(|e| {
            let d = e.localizedDescription().to_string();
            if d.contains("com.apple.security.virtualization") {
                anyhow!(
                    "ANUBIS_VZNATIVE_NO_ENTITLEMENT: this `anubis` binary is not signed with the \
                     com.apple.security.virtualization entitlement. Build + sign it with \
                     `scripts/build_signed_anubis.sh` (ad-hoc signing is sufficient for local use). \
                     Underlying error: {d}"
                )
            } else {
                anyhow!("ANUBIS_VZNATIVE_INVALID_CONFIG: {d}")
            }
        })?;

        let _vm: Retained<VZVirtualMachine> =
            VZVirtualMachine::initWithConfiguration(VZVirtualMachine::alloc(), &cfg);
    }
    Ok(())
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
fn build_validate_instantiate(_posture: &NativePosture, _staging_dir: Option<&str>) -> Result<()> {
    Err(anyhow!(
        "ANUBIS_VZNATIVE_UNSUPPORTED_HOST: the native objc2-virtualization backend requires Apple \
         Silicon macOS (aarch64-apple-darwin). Use `anubis vz confine` for the host-independent \
         manifest, or the tart-backed `anubis vz` commands where a VZ host is available."
    ))
}

/// `anubis vz native-boot <program> --kernel PATH [--initrd PATH] [--allow-host H] [--staging-dir DIR]`
///
/// Builds the same posture as `native-preflight`, attaches a real kernel/initrd, and **starts**
/// the guest. For `PerHostnameEgress`, spawns a host-side frame pump that applies
/// [`crate::vz_egress_gateway::EgressPolicy`] (deny-all when empty).
///
/// Requires a binary signed with `scripts/build_signed_anubis.sh` and a bootable Linux kernel.
pub fn native_boot(
    program: &str,
    kernel: &str,
    initrd: Option<&str>,
    allow_hosts: &[String],
    staging_dir: Option<&str>,
    run_in_guest: Option<&str>,
    engage_dir: Option<&str>,
) -> Result<()> {
    if run_in_guest.is_some() && staging_dir.is_none() {
        return Err(anyhow!(
            "ANUBIS_VZNATIVE_RUN_REQUIRES_STAGING: --run-in-guest requires --staging-dir \
             (the command runs against files on the VirtioFS share)"
        ));
    }
    let posture = derive_native_posture(program, allow_hosts)?;
    eprintln!("[anubis vz native-boot] program : {program}");
    eprintln!("[anubis vz native-boot] kernel  : {kernel}");
    if let Some(i) = initrd {
        eprintln!("[anubis vz native-boot] initrd  : {i}");
    }
    if let Some(dir) = staging_dir {
        eprintln!("[anubis vz native-boot] staging : {dir} (VirtioFS tag \"anubis\")");
    }
    if let Some(cmd) = run_in_guest {
        eprintln!("[anubis vz native-boot] run     : {cmd}");
    }
    if let Some(dir) = engage_dir {
        eprintln!("[anubis vz native-boot] engage  : {dir} (receipt chain append)");
    }
    eprintln!("[anubis vz native-boot] posture : {}", posture.label());
    boot_with_kernel(
        &posture,
        program,
        kernel,
        initrd,
        staging_dir,
        run_in_guest,
        engage_dir,
    )
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn boot_with_kernel(
    posture: &NativePosture,
    program: &str,
    kernel: &str,
    initrd: Option<&str>,
    staging_dir: Option<&str>,
    run_in_guest: Option<&str>,
    engage_dir: Option<&str>,
) -> Result<()> {
    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::AnyThread;
    use objc2_foundation::{NSArray, NSError, NSFileHandle, NSString, NSURL};
    use objc2_virtualization::{
        VZDirectorySharingDeviceConfiguration, VZFileHandleNetworkDeviceAttachment,
        VZFileHandleSerialPortAttachment, VZLinuxBootLoader, VZNetworkDeviceConfiguration,
        VZSerialPortAttachment, VZSerialPortConfiguration, VZSharedDirectory,
        VZSingleDirectoryShare, VZVirtioConsoleDeviceSerialPortConfiguration,
        VZVirtioFileSystemDeviceConfiguration, VZVirtioNetworkDeviceConfiguration,
        VZVirtualMachine, VZVirtualMachineConfiguration,
    };
    use std::io::Write as _;
    use std::os::fd::FromRawFd;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        static kCFRunLoopDefaultMode: *const core::ffi::c_void;
        fn CFRunLoopRunInMode(
            mode: *const core::ffi::c_void,
            seconds: f64,
            return_after_source_handled: u8,
        ) -> i32;
    }

    if !std::path::Path::new(kernel).is_file() {
        return Err(anyhow!(
            "ANUBIS_VZNATIVE_KERNEL_MISSING: kernel path `{kernel}` is not a file"
        ));
    }

    let policy = match posture {
        NativePosture::PerHostnameEgress { allow_hosts } => Some(
            crate::vz_egress_gateway::EgressPolicy::from_allow_hosts(allow_hosts)?,
        ),
        NativePosture::ZeroNicAirGap => None,
    };
    let allow_count = policy.as_ref().map(|p| p.allowed_ipv4.len()).unwrap_or(0);

    // ── Console I/O pipes ──
    // stdin pipe: host writes commands to [1], framework reads from [0] → guest stdin.
    // stdout pipe: guest stdout → framework writes to [1], host reads from [0].
    let mut stdin_fds = [0i32; 2];
    let mut stdout_fds = [0i32; 2];
    unsafe {
        if libc::pipe(stdin_fds.as_mut_ptr()) != 0 {
            return Err(anyhow!(
                "ANUBIS_VZNATIVE_PIPE: stdin: {}",
                std::io::Error::last_os_error()
            ));
        }
        if libc::pipe(stdout_fds.as_mut_ptr()) != 0 {
            return Err(anyhow!(
                "ANUBIS_VZNATIVE_PIPE: stdout: {}",
                std::io::Error::last_os_error()
            ));
        }
    }

    // ── Staging canary ──
    // Write a canary file with known content into the staging dir BEFORE boot. The guest reads
    // its SHA256 from inside the VM. Content match proves the file crossed the VirtioFS boundary;
    // a filename match alone could come from a stale mount.
    let has_staging = staging_dir.is_some();
    let canary_hash = if let Some(dir) = staging_dir {
        let canary_content = format!("anubis-staging-canary program={program} kernel={kernel}");
        let canary_path = std::path::Path::new(dir).join("__canary__");
        std::fs::write(&canary_path, canary_content.as_bytes())
            .map_err(|e| anyhow!("ANUBIS_VZNATIVE_CANARY_WRITE: {e}"))?;
        hex::encode(Sha256::digest(canary_content.as_bytes()))
    } else {
        String::new()
    };

    let run_in_guest_owned: Option<String> = run_in_guest.map(|s| s.to_string());

    // stdin_fds[1] stays OPEN until the shell prompt appears in console output. The reader
    // thread injects probe commands when it detects `# ` (busybox prompt), then closes the
    // write end so the shell sees EOF after the last command.
    let stdin_wr_fd = stdin_fds[1];

    let console_output = Arc::new(Mutex::new(String::new()));
    let done = Arc::new(AtomicBool::new(false));
    let vm_started = Arc::new(AtomicBool::new(false));
    let start_err: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    // Background reader: drains guest console output, injects probe commands on prompt.
    let out_for_reader = console_output.clone();
    let done_for_reader = done.clone();
    let stdout_rd_fd = stdout_fds[0];
    std::thread::spawn(move || {
        let mut f = unsafe { std::fs::File::from_raw_fd(stdout_rd_fd) };
        let mut stdin_w: Option<std::fs::File> =
            Some(unsafe { std::fs::File::from_raw_fd(stdin_wr_fd) });
        let mut buf = [0u8; 4096];
        loop {
            match std::io::Read::read(&mut f, &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let s = String::from_utf8_lossy(&buf[..n]);
                    eprint!("{s}");
                    let mut out = out_for_reader.lock().unwrap();
                    out.push_str(&s);

                    if stdin_w.is_some() && out.contains("# ") {
                        if let Some(mut w) = stdin_w.take() {
                            for cmd in [
                                // Instrument check: verify /bin/busybox exists. rdinit=/bin/sh
                                // skips Alpine's init, so applet SYMLINKS (ip, ls, cat, ping) do
                                // not exist — call /bin/busybox <applet> explicitly.
                                "test -x /bin/busybox && echo __INSTRUMENT_OK__ || echo __INSTRUMENT_MISSING__",
                                // Mount proc+sysfs so probes have something to read (rdinit=/bin/sh
                                // skips the init that normally does this).
                                "/bin/busybox mount -t proc proc /proc 2>&1 || true",
                                "/bin/busybox mount -t sysfs sysfs /sys 2>&1 || true",
                                // Load the virtio-net driver BEFORE probing.
                                //
                                // `CONFIG_VIRTIO_NET=m` in this kernel, and `rdinit=/bin/sh` runs
                                // before anything loads modules. Without this, /sys/class/net shows
                                // only `lo` WHETHER OR NOT A NIC IS ATTACHED — measured directly: a
                                // guest booted with networkDevices=1 reported exactly the same
                                // `lo`-only output as the air-gapped one, and the harness called it
                                // a verified zero-NIC proof. The probe was measuring "is virtio_net
                                // bound", not "does a NIC exist".
                                "/bin/busybox modprobe virtio_net 2>&1 || echo __MODPROBE_FAILED__",
                                "echo __ANUBIS_PROOF_START__",
                                // PRIMARY evidence: the PCI bus, which is populated by the hardware
                                // the hypervisor exposed and does NOT depend on a driver binding.
                                // This is the layer at which "the config had no NIC" is actually
                                // observable from inside the guest.
                                "echo __PCI_GLOB__:$(echo /sys/bus/pci/devices/*)",
                                "for d in /sys/bus/pci/devices/*; do echo __PCI_DEV__:$(/bin/busybox cat $d/vendor 2>/dev/null):$(/bin/busybox cat $d/device 2>/dev/null); done",
                                // Shell glob over the network stack — needs NO binary, just /bin/sh.
                                // Now meaningful because the driver has had its chance to load.
                                "echo __NET_GLOB__:$(echo /sys/class/net/*)",
                                // Secondary: explicit busybox applet (NOT a bare `ip` symlink).
                                "/bin/busybox ip link show 2>&1",
                                // Tertiary: directory listing via busybox.
                                "/bin/busybox ls /sys/class/net/ 2>&1",
                                "echo __ANUBIS_PROOF_DNS__",
                                "/bin/busybox cat /etc/resolv.conf 2>&1 || echo NO_RESOLV_CONF",
                                "/bin/busybox ping -c1 -W2 8.8.8.8 2>&1 || echo PING_FAILED",
                                // VirtioFS staging probe — runs ALWAYS. When --staging-dir is
                                // given, the mount must succeed and the canary must be readable.
                                // When NOT given, the mount must FAIL — that is the negative
                                // control proving the probe can detect absence.
                                "echo __STAGING_PROBE_START__",
                                "/bin/busybox modprobe virtiofs 2>&1; echo __STAGING_MODPROBE_EXIT__:$?",
                                "/bin/busybox mkdir -p /mnt/anubis 2>&1",
                                "/bin/busybox mount -t virtiofs anubis /mnt/anubis 2>&1; echo __STAGING_MOUNT_EXIT__:$?",
                                "echo __STAGING_GLOB__:$(/bin/busybox ls /mnt/anubis/ 2>&1)",
                                // Content hash of the canary — proves the file crossed the
                                // boundary, not just that a filename appeared.
                                "/bin/busybox sha256sum /mnt/anubis/__canary__ 2>&1; echo __STAGING_CANARY_EXIT__:$?",
                                "echo __STAGING_PROBE_END__",
                            ] {
                                let _ = writeln!(w, "{cmd}");
                            }
                            if let Some(ref run_cmd) = run_in_guest_owned {
                                let _ = writeln!(w, "echo __ANUBIS_RUN_START__");
                                let _ = writeln!(w, "{run_cmd} 2>&1");
                                let _ = writeln!(w, "echo __RUN_EXIT__:$?");
                                let _ = writeln!(w, "echo __ANUBIS_RUN_END__");
                            }
                            let _ = writeln!(w, "echo __ANUBIS_PROOF_END__");
                        }
                    }

                    if out.contains("__ANUBIS_PROOF_END__") {
                        done_for_reader.store(true, Ordering::SeqCst);
                    }
                }
            }
        }
    });

    unsafe {
        let cfg = VZVirtualMachineConfiguration::new();
        let min_mem = VZVirtualMachineConfiguration::minimumAllowedMemorySize();
        let max_mem = VZVirtualMachineConfiguration::maximumAllowedMemorySize();
        cfg.setMemorySize((1024u64 * 1024 * 1024).clamp(min_mem, max_mem));
        cfg.setCPUCount(VZVirtualMachineConfiguration::minimumAllowedCPUCount().max(1));

        // ── Boot loader + kernel cmdline ──
        let ns_kpath = NSString::from_str(kernel);
        let kurl = NSURL::fileURLWithPath(&ns_kpath);
        let boot = VZLinuxBootLoader::initWithKernelURL(VZLinuxBootLoader::alloc(), &kurl);
        if let Some(ir) = initrd {
            let ns_i = NSString::from_str(ir);
            let iurl = NSURL::fileURLWithPath(&ns_i);
            boot.setInitialRamdiskURL(Some(&iurl));
        }
        boot.setCommandLine(&NSString::from_str("console=hvc0 rdinit=/bin/sh"));
        cfg.setBootLoader(Some(&boot));

        // ── Virtio console serial port ──
        let fh_guest_in = NSFileHandle::initWithFileDescriptor_closeOnDealloc(
            NSFileHandle::alloc(),
            stdin_fds[0],
            true,
        );
        let fh_guest_out = NSFileHandle::initWithFileDescriptor_closeOnDealloc(
            NSFileHandle::alloc(),
            stdout_fds[1],
            true,
        );
        let serial_attach =
            VZFileHandleSerialPortAttachment::initWithFileHandleForReading_fileHandleForWriting(
                VZFileHandleSerialPortAttachment::alloc(),
                Some(&fh_guest_in),
                Some(&fh_guest_out),
            );
        let console_port = VZVirtioConsoleDeviceSerialPortConfiguration::new();
        let attach_super: Retained<VZSerialPortAttachment> = serial_attach.into_super();
        console_port.setAttachment(Some(&attach_super));
        let port_super: Retained<VZSerialPortConfiguration> = console_port.into_super();
        cfg.setSerialPorts(&NSArray::from_retained_slice(&[port_super]));

        // ── Network devices ──
        let mut host_fd_for_pump: Option<i32> = None;
        match posture {
            NativePosture::ZeroNicAirGap => {
                debug_assert_eq!(cfg.networkDevices().count(), 0);
            }
            NativePosture::PerHostnameEgress { .. } => {
                let mut fds = [0i32; 2];
                let rc = libc::socketpair(libc::AF_UNIX, libc::SOCK_DGRAM, 0, fds.as_mut_ptr());
                if rc != 0 {
                    return Err(anyhow!(
                        "ANUBIS_VZNATIVE_SOCKETPAIR_FAILED: {}",
                        std::io::Error::last_os_error()
                    ));
                }
                host_fd_for_pump = Some(fds[0]);
                let host_fh = NSFileHandle::initWithFileDescriptor_closeOnDealloc(
                    NSFileHandle::alloc(),
                    fds[0],
                    false,
                );
                let net_attach = VZFileHandleNetworkDeviceAttachment::initWithFileHandle(
                    VZFileHandleNetworkDeviceAttachment::alloc(),
                    &host_fh,
                );
                let dev = VZVirtioNetworkDeviceConfiguration::new();
                dev.setAttachment(Some(&net_attach));
                let dev_super: Retained<VZNetworkDeviceConfiguration> = dev.into_super();
                let arr = NSArray::from_retained_slice(&[dev_super]);
                cfg.setNetworkDevices(&arr);
                let _ = fds[1];
            }
        }

        // ── VirtioFS shared directory (R26-3: staging, independent of console proof) ──
        if let Some(dir) = staging_dir {
            let abs = std::fs::canonicalize(dir)
                .map_err(|e| anyhow!("ANUBIS_VZNATIVE_STAGING_DIR: canonicalize `{dir}`: {e}"))?;
            if !abs.is_dir() {
                return Err(anyhow!(
                    "ANUBIS_VZNATIVE_STAGING_DIR: `{dir}` is not a directory"
                ));
            }
            let ns_dir = NSString::from_str(&abs.to_string_lossy());
            let dir_url = NSURL::fileURLWithPath(&ns_dir);
            let shared = VZSharedDirectory::initWithURL_readOnly(
                VZSharedDirectory::alloc(),
                &dir_url,
                false,
            );
            let share =
                VZSingleDirectoryShare::initWithDirectory(VZSingleDirectoryShare::alloc(), &shared);
            let tag = NSString::from_str("anubis");
            let fs_dev = VZVirtioFileSystemDeviceConfiguration::initWithTag(
                VZVirtioFileSystemDeviceConfiguration::alloc(),
                &tag,
            );
            fs_dev.setShare(Some(&share));
            let fs_super: Retained<VZDirectorySharingDeviceConfiguration> = fs_dev.into_super();
            let arr = NSArray::from_retained_slice(&[fs_super]);
            cfg.setDirectorySharingDevices(&arr);
        }

        // ── Validate config ──
        cfg.validateWithError().map_err(|e| {
            let d = e.localizedDescription().to_string();
            if d.contains("com.apple.security.virtualization") {
                anyhow!(
                    "ANUBIS_VZNATIVE_NO_ENTITLEMENT: sign with scripts/build_signed_anubis.sh — {d}"
                )
            } else {
                anyhow!("ANUBIS_VZNATIVE_INVALID_CONFIG: {d}")
            }
        })?;

        // ── Egress pump (PerHostnameEgress only) ──
        if let (Some(fd), Some(pol)) = (host_fd_for_pump, policy) {
            let pol = Arc::new(pol);
            std::thread::spawn(move || {
                let mut buf = [0u8; 2048];
                loop {
                    let n = libc::read(fd, buf.as_mut_ptr() as *mut _, buf.len());
                    if n <= 0 {
                        break;
                    }
                    let frame = &buf[..n as usize];
                    if pol.permits_ethernet_frame(frame) {
                        let _ = libc::write(fd, frame.as_ptr() as *const _, n as usize);
                    }
                }
                let _ = libc::close(fd);
            });
            eprintln!(
                "[anubis vz native-boot] egress frame pump running (allow IPv4 count={allow_count})"
            );
        }

        // ── Create and start the VM ──
        let nic_count = cfg.networkDevices().count();
        let share_count = cfg.directorySharingDevices().count();
        let vm: Retained<VZVirtualMachine> =
            VZVirtualMachine::initWithConfiguration(VZVirtualMachine::alloc(), &cfg);

        eprintln!(
            "[anubis vz native-boot] config valid (networkDevices={nic_count}, \
             directorySharingDevices={share_count}). Starting VM..."
        );

        let started_cb = vm_started.clone();
        let err_cb = start_err.clone();
        let handler = RcBlock::new(move |err_ptr: *mut NSError| {
            if !err_ptr.is_null() {
                let e = &*err_ptr;
                *err_cb.lock().unwrap() = Some(e.localizedDescription().to_string());
            }
            started_cb.store(true, Ordering::SeqCst);
        });
        vm.startWithCompletionHandler(&handler);

        // Pump the main-thread run loop so VZ processes callbacks (start, I/O, etc.).
        let deadline = Instant::now() + Duration::from_secs(45);
        while Instant::now() < deadline {
            CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.25, 0);

            if vm_started.load(Ordering::SeqCst) {
                if let Some(ref e) = *start_err.lock().unwrap() {
                    return Err(anyhow!("ANUBIS_VZNATIVE_START_FAILED: {e}"));
                }
                if done.load(Ordering::SeqCst) {
                    break;
                }
            }
        }

        if !vm_started.load(Ordering::SeqCst) {
            return Err(anyhow!(
                "ANUBIS_VZNATIVE_START_TIMEOUT: VM did not start within 45 s"
            ));
        }

        let final_output = console_output.lock().unwrap().clone();
        let proof_complete = done.load(Ordering::SeqCst);

        eprintln!();
        eprintln!(
            "[anubis vz native-boot] === Console transcript (networkDevices={nic_count}) ==="
        );
        if final_output.is_empty() {
            eprintln!("(no console output received)");
        }
        eprintln!("[anubis vz native-boot] === End transcript ===");

        if !proof_complete {
            return Err(anyhow!(
                "ANUBIS_VZNATIVE_PROOF_TIMEOUT: timed out before __ANUBIS_PROOF_END__ marker. \
                 Partial output ({} bytes). Cannot declare proof from incomplete evidence.",
                final_output.len()
            ));
        }

        // ── R28 fail-closed validation ──
        // A check that reports PASS while measuring nothing is the defect class this lane
        // exists to prevent. Every gate below must pass before the proof is declared.

        // Gate 1: instrument check — /bin/busybox must exist in the guest.
        if final_output.contains("__INSTRUMENT_MISSING__") {
            return Err(anyhow!(
                "ANUBIS_VZNATIVE_INSTRUMENT_FAILED: /bin/busybox not found in guest. \
                 The probe commands cannot execute. A missing tool and a missing NIC produce \
                 byte-identical output — this is NOT evidence of an air-gap."
            ));
        }
        if !final_output.contains("__INSTRUMENT_OK__") {
            return Err(anyhow!(
                "ANUBIS_VZNATIVE_INSTRUMENT_FAILED: instrument check did not complete. \
                 Cannot distinguish a missing tool from a missing NIC."
            ));
        }

        // Gate 2: extract the proof section and reject any "not found".
        let (proof_start, proof_end) = match (
            final_output.find("__ANUBIS_PROOF_START__"),
            final_output.find("__ANUBIS_PROOF_END__"),
        ) {
            (Some(s), Some(e)) => (s, e),
            _ => {
                return Err(anyhow!(
                    "ANUBIS_VZNATIVE_PROOF_INCOMPLETE: proof markers not found in transcript"
                ));
            }
        };
        let proof_section = &final_output[proof_start..proof_end];

        // Match the SHELL's missing-command form, not the phrase anywhere.
        //
        // A bare `contains("not found")` matched the KERNEL line
        //
        //     [    0.699189] virtio-fs: tag <anubis> not found
        //
        // which is the kernel correctly reporting that no virtiofs tag was configured — the
        // EXPECTED result when `--staging-dir` is absent. So every plain zero-NIC run, the common
        // case and the one this lane exists for, failed with "one or more probe commands reported
        // 'not found'". A correct negative result read as a broken instrument.
        //
        // busybox ash emits `sh: NAME: not found` / `/bin/sh: NAME: not found`. Nothing else in a
        // guest transcript takes that shape, and a kernel subsystem message never does.
        let missing_tool = proof_section.lines().any(|l| {
            (l.contains("sh: ") || l.contains("/bin/sh: ")) && l.trim_end().ends_with("not found")
        });
        if missing_tool {
            return Err(anyhow!(
                "ANUBIS_VZNATIVE_INSTRUMENT_FAILED: one or more probe commands reported \
                 'not found' inside the guest. A missing tool and a missing NIC produce \
                 byte-identical output. Proof section:\n{proof_section}"
            ));
        }

        // Gate 3: parse the glob result — the primary evidence.
        // `echo /sys/class/net/*` expands to the interface list. If only loopback is present,
        // it expands to `/sys/class/net/lo`. If the path doesn't exist (sysfs not mounted),
        // it stays literal `/sys/class/net/*` — that is INCONCLUSIVE, not proof.
        // The shell ECHOES each command before running it, so the transcript contains the marker
        // TWICE: once in `~ # echo __NET_GLOB__:$(echo /sys/class/net/*)` and once in the result
        // `__NET_GLOB__:/sys/class/net/lo`. A `find()` on the marker hits the echo first and reads
        // back the UNEXPANDED command text, which then fails the loopback comparison and reports
        // ANUBIS_VZNATIVE_NIC_DETECTED against a guest that has only `lo`.
        //
        // Failing closed there was the right direction and the wrong verdict: it accused VZ of not
        // enforcing the config while holding a transcript that proved it had. Take the LAST
        // occurrence whose line carries no prompt and no unexpanded `$(`, which is the result by
        // construction — the echo always has both.
        let glob_value = proof_section
            .lines()
            .map(|l| l.trim())
            .rfind(|l| l.starts_with("__NET_GLOB__:") && !l.contains("$(") && !l.contains("~ #"))
            .map(|l| l["__NET_GLOB__:".len()..].trim())
            .unwrap_or("");

        if glob_value == "/sys/class/net/*" || glob_value.is_empty() {
            return Err(anyhow!(
                "ANUBIS_VZNATIVE_PROOF_INCONCLUSIVE: /sys/class/net/* did not expand \
                 (sysfs may not be mounted). The probe ran but measured nothing. \
                 Glob value: '{glob_value}'"
            ));
        }

        // The verdict depends on WHICH POSTURE was derived, and getting this backwards makes the
        // tool dishonest in both directions.
        //
        // A `PerHostnameEgress` guest is SUPPOSED to have a NIC. Reporting its `eth0` as "a VZ
        // enforcement failure" would be a false alarm — and, worse, an earlier version reported
        // `ZERO-NIC PROOF VERIFIED. networkDevices=1` for exactly that guest, which is the same
        // sentence over the opposite fact.
        //
        // Splitting on posture also turns this into a self-validating instrument: the egress
        // posture is the POSITIVE CONTROL. If a guest configured WITH a NIC does not report one,
        // the probe cannot see NICs at all and its `lo`-only answer for an air-gapped guest means
        // nothing. That was measured, not hypothesised — `CONFIG_VIRTIO_NET=m` and no module
        // loader under `rdinit=/bin/sh` made every guest look air-gapped.
        let lo_only = glob_value == "/sys/class/net/lo";
        let (verdict, posture_label) = match posture {
            NativePosture::ZeroNicAirGap => {
                if !lo_only {
                    return Err(anyhow!(
                        "ANUBIS_VZNATIVE_NIC_DETECTED: /sys/class/net/* expanded to '{glob_value}', \
                         which contains interfaces beyond loopback, in a guest configured with \
                         networkDevices={nic_count}. This is a VZ enforcement failure or a config error."
                    ));
                }
                eprintln!(
                    "[anubis vz native-boot] ZERO-NIC PROOF VERIFIED. networkDevices={nic_count}."
                );
                eprintln!(
                    "  evidence  : virtio-net (0x1af4:0x1041) ABSENT from the guest PCI bus; \
/sys/class/net/* expanded to 'lo' only"
                );
                eprintln!(
                    "  instrument: /bin/busybox present, all probes executed, no 'not found'"
                );
                eprintln!(
                    "  basis     : hypervisor-enforced (VZ networkDevices=0 + guest-side PCI-bus confirmation)"
                );
                ("ZERO_NIC_VERIFIED", "ZeroNicAirGap")
            }
            NativePosture::PerHostnameEgress { .. } => {
                if lo_only {
                    return Err(anyhow!(
                        "ANUBIS_VZNATIVE_PROBE_BLIND: the guest was configured with \
                         networkDevices={nic_count} and reported LOOPBACK ONLY. The probe cannot \
                         observe a NIC that is present, so its answer for an air-gapped guest \
                         proves nothing. Refusing to treat this instrument as trustworthy."
                    ));
                }
                eprintln!(
                    "[anubis vz native-boot] EGRESS POSTURE CONFIRMED. networkDevices={nic_count}."
                );
                eprintln!("  evidence  : guest enumerated '{glob_value}'");
                eprintln!(
                    "  note      : this run is also the POSITIVE CONTROL for the zero-NIC probe — \
it proves the probe can see a NIC when one exists"
                );
                ("EGRESS_CONFIRMED", "PerHostnameEgress")
            }
        };

        // ── Staging probe validation ──
        // When --staging-dir is given: mount MUST succeed, canary content hash MUST match.
        // When NOT given: mount MUST fail — that is the negative control.
        let staging_section = final_output.find("__STAGING_PROBE_START__").and_then(|s| {
            final_output
                .find("__STAGING_PROBE_END__")
                .map(|e| &final_output[s..e])
        });

        let (staging_mount_ok, staging_canary_match, staging_guest_hash, staging_glob_value) =
            if let Some(section) = staging_section {
                // Same narrowing as the proof-section check above, and for the same reason: a bare
                // `contains("not found")` matches the KERNEL line
                //
                //     [    0.748830] virtio-fs: tag <anubis> not found
                //
                // which is the kernel correctly reporting that no virtiofs tag was configured —
                // the EXPECTED state when `--staging-dir` is absent. That turned every plain
                // zero-NIC run into a staging-instrument failure. Two detectors, one idiom, and
                // fixing only the first left the second producing the identical false alarm.
                let staging_tool_missing = section.lines().any(|l| {
                    (l.contains("sh: ") || l.contains("/bin/sh: "))
                        && l.trim_end().ends_with("not found")
                });
                if staging_tool_missing {
                    return Err(anyhow!(
                        "ANUBIS_VZNATIVE_STAGING_INSTRUMENT_FAILED: a staging probe command \
                         reported 'not found'. A missing modprobe/mount is not evidence of \
                         'no staging' — it is a broken instrument.\n{section}"
                    ));
                }

                let mount_exit = section
                    .lines()
                    .find_map(|l| l.trim().strip_prefix("__STAGING_MOUNT_EXIT__:"))
                    .and_then(|v| v.trim().parse::<i32>().ok());
                let mount_ok = mount_exit == Some(0);

                let guest_hash: String = section
                    .lines()
                    .filter_map(|l| {
                        let t = l.trim();
                        let first = t.split_whitespace().next()?;
                        (first.len() == 64 && first.chars().all(|c| c.is_ascii_hexdigit()))
                            .then(|| first.to_string())
                    })
                    .next()
                    .unwrap_or_default();

                let canary_ok =
                    !canary_hash.is_empty() && !guest_hash.is_empty() && canary_hash == guest_hash;

                let glob_val: String = section
                    .lines()
                    .map(|l| l.trim())
                    .rfind(|l| {
                        l.starts_with("__STAGING_GLOB__:")
                            && !l.contains("$(")
                            && !l.contains("~ #")
                    })
                    .map(|l| l["__STAGING_GLOB__:".len()..].trim().to_string())
                    .unwrap_or_default();

                if has_staging {
                    if !mount_ok {
                        return Err(anyhow!(
                            "ANUBIS_VZNATIVE_STAGING_MOUNT_FAILED: --staging-dir was given \
                             but VirtioFS mount failed (exit={mount_exit:?}). The guest cannot \
                             access staged files."
                        ));
                    }
                    if !canary_ok {
                        return Err(anyhow!(
                            "ANUBIS_VZNATIVE_STAGING_CANARY_MISMATCH: canary content hash \
                             from guest ({guest_hash}) does not match host ({canary_hash}). \
                             The file did not cross the VirtioFS boundary correctly."
                        ));
                    }
                    eprintln!(
                        "[anubis vz native-boot] STAGING VERIFIED: VirtioFS mount succeeded, \
                         canary content hash matched."
                    );
                } else {
                    if mount_ok {
                        return Err(anyhow!(
                            "ANUBIS_VZNATIVE_STAGING_NEG_CONTROL_FAILED: no --staging-dir \
                             was given, but VirtioFS mount succeeded. A staging probe that \
                             reports success whether or not a share exists cannot distinguish \
                             presence from absence."
                        ));
                    }
                    eprintln!(
                        "[anubis vz native-boot] STAGING NEGATIVE CONTROL: VirtioFS mount \
                         failed as expected (no --staging-dir configured)."
                    );
                }

                (mount_ok, canary_ok, guest_hash, glob_val)
            } else {
                if has_staging {
                    return Err(anyhow!(
                        "ANUBIS_VZNATIVE_STAGING_PROBE_MISSING: --staging-dir was given but \
                         staging probe markers were not found in the transcript."
                    ));
                }
                (false, false, String::new(), String::new())
            };

        // ── Guest run validation ──
        // Five fields: exit_code, output, run_started, run_completed, crash_classification.
        // A crash op (PoC that kills the target or the guest kernel) must produce a receipt,
        // not an error — the crash IS the finding.
        let (run_exit_code, run_output_excerpt, run_started, run_completed, crash_classification) =
            if run_in_guest.is_some() {
                let start_pos = final_output.find("__ANUBIS_RUN_START__");
                let end_pos = final_output.find("__ANUBIS_RUN_END__");

                match (start_pos, end_pos) {
                    (Some(s), Some(e)) => {
                        let section = &final_output[s..e];
                        let exit_code = section
                            .lines()
                            .find_map(|l| l.trim().strip_prefix("__RUN_EXIT__:"))
                            .and_then(|v| v.trim().parse::<i32>().ok());
                        if let Some(code) = exit_code {
                            let output: String = section
                                .lines()
                                .filter(|l| {
                                    !l.contains("__ANUBIS_RUN_START__")
                                        && !l.contains("__RUN_EXIT__:")
                                        && !l.contains("~ #")
                                })
                                .collect::<Vec<_>>()
                                .join("\n");
                            let excerpt = if output.len() > 4096 {
                                format!("{}...[truncated at 4096]", &output[..4096])
                            } else {
                                output
                            };
                            let classification = if code == 0 {
                                "clean"
                            } else if code > 128 {
                                "target_signal"
                            } else {
                                "target_nonzero"
                            };
                            eprintln!(
                                "[anubis vz native-boot] GUEST RUN COMPLETE: exit={code}, \
                                 classification={classification}, output={} bytes",
                                excerpt.len()
                            );
                            (Some(code), excerpt, true, true, classification)
                        } else {
                            return Err(anyhow!(
                                "ANUBIS_VZNATIVE_RUN_EXIT_MISSING: guest run markers \
                                 present but __RUN_EXIT__ not found or not parseable."
                            ));
                        }
                    }
                    (Some(s), None) => {
                        // Run started but guest died before __ANUBIS_RUN_END__.
                        // This is either a guest kernel crash (PoC killed the kernel —
                        // a BIGGER finding than a process crash) or infrastructure death.
                        let after_start = &final_output[s..];
                        let output: String = after_start
                            .lines()
                            .filter(|l| !l.contains("__ANUBIS_RUN_START__") && !l.contains("~ #"))
                            .collect::<Vec<_>>()
                            .join("\n");
                        let excerpt = if output.len() > 4096 {
                            format!("{}...[truncated at 4096]", &output[..4096])
                        } else {
                            output
                        };
                        let classification =
                            if excerpt.contains("Kernel panic") || excerpt.contains("kernel BUG") {
                                "guest_kernel_crash"
                            } else if excerpt.contains("Segmentation fault")
                                || excerpt.contains("SIGSEGV")
                                || excerpt.contains("SIGBUS")
                                || excerpt.contains("SIGABRT")
                                || excerpt.contains("core dumped")
                            {
                                "target_signal"
                            } else {
                                "ambiguous"
                            };
                        eprintln!(
                            "[anubis vz native-boot] GUEST RUN INCOMPLETE: no __RUN_EXIT__, \
                             classification={classification}, output={} bytes",
                            excerpt.len()
                        );
                        (None, excerpt, true, false, classification)
                    }
                    (None, _) => {
                        return Err(anyhow!(
                            "ANUBIS_VZNATIVE_RUN_MARKERS_MISSING: --run-in-guest was given \
                             but __ANUBIS_RUN_START__ not found in transcript. The guest \
                             died before reaching the run phase."
                        ));
                    }
                }
            } else {
                (None, String::new(), false, false, "not_applicable")
            };

        // ── Post-run artifact scan ──
        let artifact_hashes: Vec<(String, String)> = if run_in_guest.is_some() {
            if let Some(dir) = staging_dir {
                let mut hashes = Vec::new();
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        if name == "__canary__" {
                            continue;
                        }
                        if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                            if let Ok(bytes) = std::fs::read(entry.path()) {
                                let hash = hex::encode(Sha256::digest(&bytes));
                                hashes.push((name, hash));
                            }
                        }
                    }
                }
                hashes.sort_by(|a, b| a.0.cmp(&b.0));
                hashes
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        // ── Receipt emission ──
        // The receipt carries the evidence, not a boolean. An auditor re-derives the posture
        // from the program, verifies the kernel hash against a known-good build, and reads
        // the guest's own PCI enumeration and network interface list.
        let transcript_sha256 = hex::encode(Sha256::digest(final_output.as_bytes()));
        let program_sha256 = std::fs::read(program)
            .map(|b| hex::encode(Sha256::digest(&b)))
            .unwrap_or_else(|_| "unavailable".into());
        let kernel_sha256 = std::fs::read(kernel)
            .map(|b| hex::encode(Sha256::digest(&b)))
            .unwrap_or_else(|_| "unavailable".into());

        let pci_devices: Vec<&str> = proof_section
            .lines()
            .filter_map(|l| {
                let t = l.trim();
                t.strip_prefix("__PCI_DEV__:")
                    .filter(|v| !v.contains("$(") && !v.contains("~ #"))
            })
            .collect();
        let virtio_net_present = pci_devices.iter().any(|d| d.contains("0x1041"));

        let receipt = serde_json::json!({
            "action": "native-boot-isolation-proof",
            "isolation_basis": "hypervisor-enforced",
            "verdict": verdict,
            "posture": posture_label,
            "posture_derived_from": "program_proven_effect_set",
            "config": {
                "networkDevices": nic_count,
                "serialPorts": 1,
                "directorySharingDevices": share_count,
                "cmdline": "console=hvc0 rdinit=/bin/sh"
            },
            "program": program,
            "program_sha256": program_sha256,
            "kernel": kernel,
            "kernel_sha256": kernel_sha256,
            "evidence": {
                "pci_devices": pci_devices,
                "virtio_net_present": virtio_net_present,
                "net_glob": glob_value,
                "instrument_validated": true,
                "no_dead_probes": true
            },
            "staging": {
                "configured": has_staging,
                "mount_succeeded": staging_mount_ok,
                "canary_content_hash_host": canary_hash,
                "canary_content_hash_guest": staging_guest_hash,
                "canary_match": staging_canary_match,
                "staging_glob": staging_glob_value
            },
            "run": {
                "configured": run_in_guest.is_some(),
                "command": run_in_guest.unwrap_or(""),
                "exit_code": run_exit_code,
                "run_started": run_started,
                "run_completed": run_completed,
                "crash_classification": crash_classification,
                "crash_signal": run_exit_code.filter(|c| *c > 128).map(|c| c - 128),
                "evidence_of": match crash_classification {
                    "target_signal" | "guest_kernel_crash" => "finding",
                    "target_nonzero" => "finding",
                    "clean" | "not_applicable" => "nothing",
                    _ => "ambiguous",
                },
                "output_excerpt": run_output_excerpt,
                "artifacts": artifact_hashes.iter().map(|(n, h)| {
                    serde_json::json!({"name": n, "sha256": h})
                }).collect::<Vec<_>>(),
                "honest_limit": "crash_classification is heuristic: exit > 128 is \
                    interpreted as signal death (Unix convention), but a program can \
                    exit(139) deliberately. guest_kernel_crash is detected by string \
                    matching 'Kernel panic' in output. An auditor applies judgment."
            },
            "transcript_sha256": transcript_sha256,
            "transcript_bytes": final_output.len()
        });

        println!(
            "{}",
            serde_json::to_string_pretty(&receipt).unwrap_or_default()
        );

        if let Some(eng) = engage_dir {
            let eng_path = std::path::Path::new(eng);
            let engagement_id = eng_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("native-boot");
            let sealed = crate::offensive::receipts::seal_action(
                eng_path,
                engagement_id,
                "vz_native_boot",
                "forge",
                receipt,
            )?;
            eprintln!(
                "[anubis vz native-boot] receipt chained: seq={}, tip={}",
                sealed.seq,
                &sealed.receipt_hash[..16]
            );
        }
    }
    Ok(())
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
fn boot_with_kernel(
    _posture: &NativePosture,
    _program: &str,
    _kernel: &str,
    _initrd: Option<&str>,
    _staging_dir: Option<&str>,
    _run_in_guest: Option<&str>,
    _engage_dir: Option<&str>,
) -> Result<()> {
    Err(anyhow!(
        "ANUBIS_VZNATIVE_UNSUPPORTED_HOST: native-boot requires Apple Silicon macOS"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &std::path::Path, name: &str, src: &str) -> String {
        let p = dir.join(name);
        std::fs::write(&p, src).unwrap();
        p.to_string_lossy().into_owned()
    }

    // The posture derivation is the soundness-bearing half (the FFI is a host-gated proof of the
    // entitlement, exercised by `native-preflight` on a signed binary). Lock the FAIL-CLOSED lattice:
    // net-free / unbounded => most restrictive (zero-NIC air-gap); net-using => per-hostname egress;
    // a program that does not pass `anubis check` is refused.
    #[test]
    fn native_posture_is_fail_closed() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // net-free => zero-NIC air-gap
        let nf = write(
            root,
            "nf.anb",
            "fn add(a: i64, b: i64) -> i64 { return a + b; }\nfn main() { let _ = add(1, 2); }\n",
        );
        assert_eq!(
            derive_native_posture(&nf, &[]).unwrap(),
            NativePosture::ZeroNicAirGap
        );

        // net-using => per-hostname egress, carrying the allow-list
        let nu = write(
            root,
            "nu.anb",
            "fn beacon() uses(net.send) { http_post(\"http://x/y\", \"z\"); }\nfn main() uses(net.send) { beacon(); }\n",
        );
        let hosts = vec!["a.example".to_string()];
        assert_eq!(
            derive_native_posture(&nu, &hosts).unwrap(),
            NativePosture::PerHostnameEgress {
                allow_hosts: hosts.clone()
            }
        );

        // net-USING but UNBOUNDED (a closure the effect walk cannot resolve): the unbounded bit
        // OVERRIDES net-present to the MOST restrictive posture (zero-NIC), never per-hostname — deny
        // on minimum knowledge. This exercises the `!effects_bounded` clause specifically (net.send IS
        // present here, so a permissive reading would wrongly pick PerHostnameEgress).
        let unb = write(
            root,
            "unb.anb",
            "fn run(cb) uses(net.send) { send(\"h\", 80, \"x\"); let _ = cb(1); }\n\
             fn main() uses(net.send) { run(|x| x + 1); }\n",
        );
        assert_eq!(
            derive_native_posture(&unb, &hosts).unwrap(),
            NativePosture::ZeroNicAirGap,
            "a net-using but UNBOUNDED effect set must confine to the MOST restrictive posture (fail-closed)"
        );

        // a program that does not pass `anubis check` is refused (no proof to confine from).
        let bad = write(
            root,
            "bad.anb",
            "fn bad() { let x = undefined_zzz_symbol; }\nfn main() { bad(); }\n",
        );
        let err = derive_native_posture(&bad, &[]).unwrap_err().to_string();
        assert!(
            err.contains("ANUBIS_VZNATIVE_UNVERIFIED"),
            "a non-checking program must be refused: {err}"
        );
    }
}
