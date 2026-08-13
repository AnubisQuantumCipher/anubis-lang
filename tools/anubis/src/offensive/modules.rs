//! Operator-side and agent-side module catalog.

use anyhow::Result;
use serde_json::json;

#[derive(Debug, Clone)]
pub struct ModuleInfo {
    pub name: &'static str,
    pub side: &'static str, // agent | operator | both
    pub risk: &'static str,
    pub description: &'static str,
}

pub fn catalog() -> Vec<ModuleInfo> {
    vec![
        ModuleInfo {
            name: "whoami",
            side: "agent",
            risk: "low",
            description: "Current user identity",
        },
        ModuleInfo {
            name: "hostname",
            side: "agent",
            risk: "low",
            description: "Hostname",
        },
        ModuleInfo {
            name: "pwd",
            side: "agent",
            risk: "low",
            description: "Working directory",
        },
        ModuleInfo {
            name: "id",
            side: "agent",
            risk: "low",
            description: "POSIX id",
        },
        ModuleInfo {
            name: "uname",
            side: "agent",
            risk: "low",
            description: "Kernel/uname -a",
        },
        ModuleInfo {
            name: "ls",
            side: "agent",
            risk: "low",
            description: "Directory listing (scoped by engagement paths when operator-gated)",
        },
        ModuleInfo {
            name: "cat",
            side: "agent",
            risk: "medium",
            description: "Read file contents",
        },
        ModuleInfo {
            name: "sleep",
            side: "agent",
            risk: "low",
            description: "Sleep milliseconds",
        },
        ModuleInfo {
            name: "die",
            side: "agent",
            risk: "low",
            description: "Agent self-exit",
        },
        ModuleInfo {
            name: "target_run",
            side: "operator",
            risk: "high",
            description: "Operator PoC kit process harness against in-scope local binary",
        },
        ModuleInfo {
            name: "fuzz",
            side: "operator",
            risk: "high",
            description: "Operator mutation fuzz against in-scope local binary",
        },
        ModuleInfo {
            name: "exploit_run",
            side: "operator",
            risk: "critical",
            description: "Run an exploit module JSON against in-scope target",
        },
        ModuleInfo {
            name: "persist_launchagent",
            side: "operator",
            risk: "high",
            description: "Generate macOS LaunchAgent plist for lab agent (T2)",
        },
        ModuleInfo {
            name: "inject_plan",
            side: "operator",
            risk: "critical",
            description: "Process inject: PLAN_ONLY default; live under double auth (T2)",
        },
        ModuleInfo {
            name: "lateral_ssh",
            side: "operator",
            risk: "high",
            description: "SSH to host in allowed_lateral_hosts (T4)",
        },
        ModuleInfo {
            name: "lateral_smb",
            side: "operator",
            risk: "high",
            description: "SMB/WinRM lateral PLAN_ONLY (T4) — emits plan, never executes",
        },
        ModuleInfo {
            name: "string_scramble",
            side: "operator",
            risk: "low",
            description: "Lab XOR string scramble for stub notes (T6)",
        },
        ModuleInfo {
            name: "pattern_create",
            side: "operator",
            risk: "low",
            description: "Cyclic pattern for overflow offset (T5)",
        },
        ModuleInfo {
            name: "pattern_offset",
            side: "operator",
            risk: "low",
            description: "Find offset in cyclic pattern (T5)",
        },
        ModuleInfo {
            name: "gadget_search",
            side: "operator",
            risk: "medium",
            description: "Search user-supplied gadget list (T5)",
        },
        ModuleInfo {
            name: "browser_harness",
            side: "operator",
            risk: "medium",
            description: "Localhost browser chain harness HTML (T5)",
        },
        ModuleInfo {
            name: "xor_pack",
            side: "operator",
            risk: "medium",
            description: "Lab XOR packer + C unpack stub (T6)",
        },
        ModuleInfo {
            name: "attck_catalog",
            side: "operator",
            risk: "low",
            description: "MITRE ATT&CK kill-chain catalog mapped to AOP (T9)",
        },
        ModuleInfo {
            name: "opsec_score",
            side: "operator",
            risk: "low",
            description: "Engagement OPSEC score + elite checklist (T9)",
        },
        ModuleInfo {
            name: "recon_hostinfo",
            side: "operator",
            risk: "low",
            description: "Local operator + engagement scope recon facts (T9)",
        },
        ModuleInfo {
            name: "recon_scan",
            side: "operator",
            risk: "medium",
            description: "Scoped port recon — VZ guest only (T9)",
        },
        ModuleInfo {
            name: "malleable_profile",
            side: "operator",
            risk: "low",
            description: "Malleable C2 HTTP profile init/validate (T9)",
        },
        ModuleInfo {
            name: "campaign_playbook",
            side: "operator",
            risk: "low",
            description: "Full-spectrum campaign playbook JSON/MD (T9)",
        },
        ModuleInfo {
            name: "purple_report",
            side: "operator",
            risk: "low",
            description: "Purple-team ATT&CK coverage + detection gaps (T9)",
        },
        ModuleInfo {
            name: "phish_plan",
            side: "operator",
            risk: "medium",
            description: "Phishing campaign PLAN_ONLY — never sends (T9)",
        },
        ModuleInfo {
            name: "lolbas_catalog",
            side: "operator",
            risk: "low",
            description: "Living-off-the-land technique catalog PLAN_ONLY (T9)",
        },
        ModuleInfo {
            name: "vz_status",
            side: "operator",
            risk: "low",
            description: "Apple VZ guest status (T8)",
        },
        ModuleInfo {
            name: "vz_doctor",
            side: "operator",
            risk: "low",
            description: "VZ sandbox readiness check (T8)",
        },
        ModuleInfo {
            name: "vz_exec",
            side: "operator",
            risk: "high",
            description: "Execute inside a crash-isolated Tart guest — shared NAT (T8)",
        },
        ModuleInfo {
            name: "vz_exploit",
            side: "operator",
            risk: "critical",
            description: "Run exploit module inside VZ sandbox (T8)",
        },
        ModuleInfo {
            name: "vz_fuzz",
            side: "operator",
            risk: "high",
            description: "Fuzz target inside VZ guest — no host crash risk (T8)",
        },
        ModuleInfo {
            name: "vz_agent_test",
            side: "operator",
            risk: "high",
            description: "Build + test agent binary inside VZ (T8)",
        },
        ModuleInfo {
            name: "vz_c2_cycle",
            side: "operator",
            risk: "critical",
            description: "Full C2 lifecycle inside VZ: listener + agent + tasks (T8)",
        },
        ModuleInfo {
            name: "vz_stress",
            side: "operator",
            risk: "critical",
            description: "Full offensive stress battery inside VZ (T8)",
        },
        // ── Credential Access (T10) ──
        ModuleInfo {
            name: "credential_hash_test",
            side: "operator",
            risk: "medium",
            description: "Offline hash cracking against wordlist (T1110.002)",
        },
        ModuleInfo {
            name: "credential_ssh_key_audit",
            side: "operator",
            risk: "low",
            description: "Audit SSH keys for weak permissions (T1552.004)",
        },
        ModuleInfo {
            name: "credential_spray_plan",
            side: "operator",
            risk: "medium",
            description: "Credential spray planning — PLAN_ONLY (T1110.003)",
        },
        ModuleInfo {
            name: "credential_env_scan",
            side: "operator",
            risk: "low",
            description: "Scan env vars for credential patterns (T1552.001)",
        },
        ModuleInfo {
            name: "credential_keychain_plan",
            side: "operator",
            risk: "medium",
            description: "macOS keychain enumeration — PLAN_ONLY (T1555.001)",
        },
        // ── Privilege Escalation (T10) ──
        ModuleInfo {
            name: "privesc_suid_enum",
            side: "operator",
            risk: "low",
            description: "Find SUID/SGID binaries (T1548.001)",
        },
        ModuleInfo {
            name: "privesc_sudo_audit",
            side: "operator",
            risk: "low",
            description: "Audit sudo configuration (T1548.003)",
        },
        ModuleInfo {
            name: "privesc_writable_path",
            side: "operator",
            risk: "low",
            description: "Find writable PATH directories (T1574.007)",
        },
        ModuleInfo {
            name: "privesc_cron_enum",
            side: "operator",
            risk: "low",
            description: "Enumerate cron jobs + LaunchDaemons (T1053.003)",
        },
        ModuleInfo {
            name: "privesc_kernel_plan",
            side: "operator",
            risk: "high",
            description: "Kernel exploit planning — PLAN_ONLY (T1068)",
        },
        ModuleInfo {
            name: "privesc_enum",
            side: "operator",
            risk: "medium",
            description: "Full privilege escalation enumeration (TA0004)",
        },
        // ── Discovery (T10) ──
        ModuleInfo {
            name: "discovery_system_enum",
            side: "operator",
            risk: "low",
            description: "System info: uname, users, processes (T1082/T1033)",
        },
        ModuleInfo {
            name: "discovery_network_enum",
            side: "operator",
            risk: "low",
            description: "Network interfaces, routing, ARP, DNS (T1016/T1049)",
        },
        ModuleInfo {
            name: "discovery_process_enum",
            side: "operator",
            risk: "low",
            description: "Running processes with security-relevant matches (T1057)",
        },
        ModuleInfo {
            name: "discovery_file_discovery",
            side: "operator",
            risk: "medium",
            description: "Find sensitive files: keys, configs, databases (T1083/T1005)",
        },
        ModuleInfo {
            name: "discovery_service_banner",
            side: "operator",
            risk: "medium",
            description: "Service banner grabbing on in-scope hosts (T1046)",
        },
        ModuleInfo {
            name: "discovery_cloud_metadata_plan",
            side: "operator",
            risk: "medium",
            description: "Cloud metadata probe — PLAN_ONLY (T1552.005)",
        },
        ModuleInfo {
            name: "discovery_ad_enum_plan",
            side: "operator",
            risk: "medium",
            description: "Active Directory enumeration — PLAN_ONLY (T1087.002)",
        },
        // ── Collection (T10) ──
        ModuleInfo {
            name: "collection_clipboard",
            side: "operator",
            risk: "medium",
            description: "Clipboard capture via pbpaste/xclip (T1115)",
        },
        ModuleInfo {
            name: "collection_stage_files",
            side: "operator",
            risk: "medium",
            description: "Stage files to engagement loot (T1074.001)",
        },
        ModuleInfo {
            name: "collection_screen_plan",
            side: "operator",
            risk: "medium",
            description: "Screen capture — PLAN_ONLY (T1113)",
        },
        ModuleInfo {
            name: "collection_keylog_plan",
            side: "operator",
            risk: "high",
            description: "Keylogging — PLAN_ONLY (T1056.001)",
        },
        ModuleInfo {
            name: "collection_archive_loot",
            side: "operator",
            risk: "low",
            description: "Archive engagement loot as evidence bundle (T1560.001)",
        },
        // ── Defense Evasion (T10) ──
        ModuleInfo {
            name: "evasion_security_enum",
            side: "operator",
            risk: "low",
            description: "Detect installed AV/EDR products (T1518.001)",
        },
        ModuleInfo {
            name: "evasion_timestomp_plan",
            side: "operator",
            risk: "medium",
            description: "Timestamp manipulation — PLAN_ONLY (T1070.006)",
        },
        ModuleInfo {
            name: "evasion_log_clear_plan",
            side: "operator",
            risk: "medium",
            description: "Log clearing — PLAN_ONLY (T1070.001)",
        },
        ModuleInfo {
            name: "evasion_amsi_plan",
            side: "operator",
            risk: "high",
            description: "AMSI/ETW bypass — PLAN_ONLY (T1562.001)",
        },
        ModuleInfo {
            name: "evasion_hollowing_plan",
            side: "operator",
            risk: "critical",
            description: "Process hollowing/injection — PLAN_ONLY (T1055.012)",
        },
        ModuleInfo {
            name: "evasion_codesign_check",
            side: "operator",
            risk: "low",
            description: "Binary code signature check (T1553.002)",
        },
        ModuleInfo {
            name: "evasion_assessment",
            side: "operator",
            risk: "medium",
            description: "Full defense evasion assessment (TA0005)",
        },
        // ── Exfiltration (T10) ──
        ModuleInfo {
            name: "exfil_dns_encode",
            side: "operator",
            risk: "medium",
            description: "DNS exfiltration encoding — no queries sent (T1048.003)",
        },
        ModuleInfo {
            name: "exfil_http_stage",
            side: "operator",
            risk: "medium",
            description: "HTTP exfiltration staging — no data transmitted (T1048.002)",
        },
        ModuleInfo {
            name: "exfil_stego_plan",
            side: "operator",
            risk: "medium",
            description: "Steganography — PLAN_ONLY (T1027.003)",
        },
        ModuleInfo {
            name: "exfil_tunnel_plan",
            side: "operator",
            risk: "medium",
            description: "Protocol tunneling — PLAN_ONLY (T1572)",
        },
        // ── Infrastructure ──
        ModuleInfo {
            name: "infra_c2_check",
            side: "operator",
            risk: "low",
            description: "C2 listener port availability check (T1071.001)",
        },
        ModuleInfo {
            name: "infra_c2_guide",
            side: "operator",
            risk: "low",
            description: "C2 framework comparison guide (T1219)",
        },
        ModuleInfo {
            name: "infra_redirector_plan",
            side: "operator",
            risk: "low",
            description: "Redirector architecture — PLAN_ONLY (T1090.002)",
        },
        ModuleInfo {
            name: "infra_domain_fronting_plan",
            side: "operator",
            risk: "low",
            description: "Domain fronting analysis — PLAN_ONLY (T1090.004)",
        },
        ModuleInfo {
            name: "infra_health",
            side: "operator",
            risk: "low",
            description: "Infrastructure health check (ports, connectivity)",
        },
        // ── Post-Exploitation ──
        ModuleInfo {
            name: "postex_persistence_enum",
            side: "operator",
            risk: "medium",
            description: "Persistence vector enumeration (TA0003)",
        },
        ModuleInfo {
            name: "postex_persistence_plan",
            side: "operator",
            risk: "high",
            description: "Persistence implant — PLAN_ONLY (T1546/T1543)",
        },
        ModuleInfo {
            name: "postex_cleanup",
            side: "operator",
            risk: "low",
            description: "Engagement cleanup checklist (T1070)",
        },
        // ── Payloads ──
        ModuleInfo {
            name: "payload_cyclic",
            side: "operator",
            risk: "low",
            description: "Cyclic pattern for crash offset identification (T1203)",
        },
        ModuleInfo {
            name: "payload_offset",
            side: "operator",
            risk: "low",
            description: "Find offset in cyclic pattern (T1203)",
        },
        ModuleInfo {
            name: "payload_encode",
            side: "operator",
            risk: "medium",
            description: "Payload encoding for AV detection testing (T1027)",
        },
        ModuleInfo {
            name: "payload_shellcode_plan",
            side: "operator",
            risk: "high",
            description: "Shellcode generation — PLAN_ONLY (T1059.004)",
        },
        ModuleInfo {
            name: "payload_delivery_plan",
            side: "operator",
            risk: "medium",
            description: "Delivery method planning — PLAN_ONLY (T1566)",
        },
        // ── Reporting ──
        ModuleInfo {
            name: "report_executive",
            side: "operator",
            risk: "low",
            description: "Executive summary report generation",
        },
        ModuleInfo {
            name: "report_technical",
            side: "operator",
            risk: "low",
            description: "Technical report with categorized findings",
        },
        ModuleInfo {
            name: "report_attck_coverage",
            side: "operator",
            risk: "low",
            description: "ATT&CK coverage matrix report",
        },
        ModuleInfo {
            name: "report_markdown",
            side: "operator",
            risk: "low",
            description: "Markdown report generation",
        },
    ]
}

pub fn list_json() -> serde_json::Value {
    let mods: Vec<_> = catalog()
        .into_iter()
        .map(|m| {
            json!({
                "name": m.name,
                "side": m.side,
                "risk": m.risk,
                "description": m.description,
            })
        })
        .collect();
    json!({
        "schema_version": "1.0",
        "platform": "anubis-offensive",
        "modules": mods,
    })
}

pub fn print_catalog() -> Result<()> {
    println!("{:<16} {:<10} {:<10} DESCRIPTION", "MODULE", "SIDE", "RISK");
    println!("{}", "-".repeat(72));
    for m in catalog() {
        println!(
            "{:<16} {:<10} {:<10} {}",
            m.name, m.side, m.risk, m.description
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_nonempty() {
        let cat = catalog();
        assert!(!cat.is_empty(), "catalog must not be empty");
    }

    #[test]
    fn catalog_no_duplicate_names() {
        let cat = catalog();
        let mut names: Vec<&str> = cat.iter().map(|m| m.name).collect();
        names.sort();
        let before = names.len();
        names.dedup();
        assert_eq!(names.len(), before, "duplicate module names in catalog");
    }

    #[test]
    fn tart_module_descriptions_do_not_claim_network_isolation() {
        for module in catalog()
            .into_iter()
            .filter(|module| module.name.starts_with("vz_"))
        {
            let description = module.description.to_ascii_lowercase();
            assert!(
                !description.contains("network isolated") && !description.contains("no egress"),
                "{} overclaims Tart isolation: {}",
                module.name,
                module.description
            );
        }
        let exec = catalog()
            .into_iter()
            .find(|module| module.name == "vz_exec")
            .expect("vz_exec catalog entry");
        assert!(
            exec.description.contains("shared NAT"),
            "{}",
            exec.description
        );
    }

    #[test]
    fn catalog_risk_values_are_valid() {
        let valid = ["low", "medium", "high", "critical"];
        for m in catalog() {
            assert!(
                valid.contains(&m.risk),
                "module {} has invalid risk {:?} — expected one of {:?}",
                m.name,
                m.risk,
                valid
            );
        }
    }

    #[test]
    fn catalog_side_values_are_valid() {
        let valid = ["agent", "operator", "both"];
        for m in catalog() {
            assert!(
                valid.contains(&m.side),
                "module {} has invalid side {:?} — expected one of {:?}",
                m.name,
                m.side,
                valid
            );
        }
    }

    #[test]
    fn list_json_round_trips_catalog() {
        let cat = catalog();
        let v = list_json();
        assert_eq!(v["schema_version"].as_str(), Some("1.0"));
        let mods = v["modules"].as_array().unwrap();
        assert_eq!(
            mods.len(),
            cat.len(),
            "list_json module count must equal catalog count"
        );
        for (i, m) in mods.iter().enumerate() {
            assert_eq!(
                m["name"].as_str(),
                Some(cat[i].name),
                "module {i} name mismatch in round-trip"
            );
        }
    }
}
