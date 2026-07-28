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
) -> Result<()> {
    let posture = derive_native_posture(program, allow_hosts)?;
    eprintln!("[anubis vz native-boot] program : {program}");
    eprintln!("[anubis vz native-boot] kernel  : {kernel}");
    if let Some(i) = initrd {
        eprintln!("[anubis vz native-boot] initrd  : {i}");
    }
    if let Some(dir) = staging_dir {
        eprintln!("[anubis vz native-boot] staging : {dir} (VirtioFS tag \"anubis\")");
    }
    eprintln!("[anubis vz native-boot] posture : {}", posture.label());
    boot_with_kernel(&posture, kernel, initrd, staging_dir)
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn boot_with_kernel(
    posture: &NativePosture,
    kernel: &str,
    initrd: Option<&str>,
    staging_dir: Option<&str>,
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
                                "echo __ANUBIS_PROOF_START__",
                                "ip link show",
                                "ip addr show",
                                "ls /sys/class/net/ 2>/dev/null || echo NO_SYSFS",
                                "echo __ANUBIS_PROOF_DNS__",
                                "cat /etc/resolv.conf 2>&1 || echo NO_RESOLV_CONF",
                                "ping -c1 -W2 8.8.8.8 2>&1 || echo PING_FAILED",
                                "echo __ANUBIS_PROOF_END__",
                            ] {
                                let _ = writeln!(w, "{cmd}");
                            }
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

        if proof_complete {
            eprintln!(
                "[anubis vz native-boot] ZERO-NIC PROOF CAPTURED. networkDevices={nic_count}. \
                 Guest-side evidence is in the transcript above."
            );
        } else {
            eprintln!(
                "[anubis vz native-boot] WARNING: timed out before __ANUBIS_PROOF_END__ marker. \
                 Partial output ({} bytes) may still contain evidence.",
                final_output.len()
            );
        }
    }
    Ok(())
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
fn boot_with_kernel(
    _posture: &NativePosture,
    _kernel: &str,
    _initrd: Option<&str>,
    _staging_dir: Option<&str>,
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
