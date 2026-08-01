//! MITRE ATT&CK kill-chain catalog for AOP (lab / authorized engagements).
//! Maps operator modules and campaign phases to technique IDs for purple-team reporting.

use serde::{Deserialize, Serialize};
use serde_json::json;

/// ATT&CK tactics (kill-chain order used by elite red teams).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tactic {
    Reconnaissance,
    ResourceDevelopment,
    InitialAccess,
    Execution,
    Persistence,
    PrivilegeEscalation,
    DefenseEvasion,
    CredentialAccess,
    Discovery,
    LateralMovement,
    Collection,
    CommandAndControl,
    Exfiltration,
    Impact,
}

impl Tactic {
    pub fn id(self) -> &'static str {
        match self {
            Tactic::Reconnaissance => "TA0043",
            Tactic::ResourceDevelopment => "TA0042",
            Tactic::InitialAccess => "TA0001",
            Tactic::Execution => "TA0002",
            Tactic::Persistence => "TA0003",
            Tactic::PrivilegeEscalation => "TA0004",
            Tactic::DefenseEvasion => "TA0005",
            Tactic::CredentialAccess => "TA0006",
            Tactic::Discovery => "TA0007",
            Tactic::LateralMovement => "TA0008",
            Tactic::Collection => "TA0009",
            Tactic::CommandAndControl => "TA0011",
            Tactic::Exfiltration => "TA0010",
            Tactic::Impact => "TA0040",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Tactic::Reconnaissance => "Reconnaissance",
            Tactic::ResourceDevelopment => "Resource Development",
            Tactic::InitialAccess => "Initial Access",
            Tactic::Execution => "Execution",
            Tactic::Persistence => "Persistence",
            Tactic::PrivilegeEscalation => "Privilege Escalation",
            Tactic::DefenseEvasion => "Defense Evasion",
            Tactic::CredentialAccess => "Credential Access",
            Tactic::Discovery => "Discovery",
            Tactic::LateralMovement => "Lateral Movement",
            Tactic::Collection => "Collection",
            Tactic::CommandAndControl => "Command and Control",
            Tactic::Exfiltration => "Exfiltration",
            Tactic::Impact => "Impact",
        }
    }

    pub fn order(self) -> u8 {
        match self {
            Tactic::Reconnaissance => 1,
            Tactic::ResourceDevelopment => 2,
            Tactic::InitialAccess => 3,
            Tactic::Execution => 4,
            Tactic::Persistence => 5,
            Tactic::PrivilegeEscalation => 6,
            Tactic::DefenseEvasion => 7,
            Tactic::CredentialAccess => 8,
            Tactic::Discovery => 9,
            Tactic::LateralMovement => 10,
            Tactic::Collection => 11,
            Tactic::CommandAndControl => 12,
            Tactic::Exfiltration => 13,
            Tactic::Impact => 14,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Technique {
    pub id: String,
    pub name: String,
    pub tactic: Tactic,
    /// AOP surface that exercises this technique (module or CLI).
    pub aop_surface: String,
    /// safe | plan_only | live_double_auth | research_vz
    pub execution_mode: String,
    pub notes: String,
}

/// Curated catalog: techniques elite red teams actually chain, mapped to AOP.
pub fn catalog() -> Vec<Technique> {
    vec![
        t(
            "T1595",
            "Active Scanning",
            Tactic::Reconnaissance,
            "recon-scan",
            "live_scoped",
            "Scoped port/service probe of in-engagement hosts only",
        ),
        t(
            "T1592",
            "Gather Victim Host Information",
            Tactic::Reconnaissance,
            "recon-hostinfo",
            "live_scoped",
            "Hostname/OS fingerprint within scope",
        ),
        t(
            "T1583",
            "Acquire Infrastructure",
            Tactic::ResourceDevelopment,
            "engage-init",
            "safe",
            "Engagement workspace + cert material (lab C2 infra)",
        ),
        t(
            "T1566",
            "Phishing",
            Tactic::InitialAccess,
            "phish-plan",
            "plan_only",
            "Social-engineering campaign plan; never sends email",
        ),
        t(
            "T1203",
            "Exploitation for Client Execution",
            Tactic::Execution,
            "exploit-run / vz-exploit",
            "research_vz",
            "Crash PoC against in-scope local target; VZ mandatory",
        ),
        t(
            "T1059",
            "Command and Scripting Interpreter",
            Tactic::Execution,
            "agent:whoami/id/uname",
            "live_scoped",
            "whoami/id/uname style discovery tasks on beacon",
        ),
        t(
            "T1543.001",
            "Launch Agent",
            Tactic::Persistence,
            "persist-launchagent",
            "safe",
            "macOS LaunchAgent artifact generation (install is human)",
        ),
        t(
            "T1055",
            "Process Injection",
            Tactic::DefenseEvasion,
            "inject-plan",
            "plan_only_or_double_auth",
            "PLAN_ONLY default; live under double authorization",
        ),
        t(
            "T1027",
            "Obfuscated Files or Information",
            Tactic::DefenseEvasion,
            "pack-xor / string-scramble",
            "safe",
            "Lab packer / scramble — not production AV evasion claim",
        ),
        t(
            "T1082",
            "System Information Discovery",
            Tactic::Discovery,
            "agent:uname/hostname",
            "live_scoped",
            "Beacon discovery modules",
        ),
        t(
            "T1083",
            "File and Directory Discovery",
            Tactic::Discovery,
            "agent:ls/pwd",
            "live_scoped",
            "Path-scoped listing",
        ),
        t(
            "T1021.004",
            "SSH",
            Tactic::LateralMovement,
            "lateral-ssh",
            "live_scoped",
            "Fail-closed to allowed_lateral_hosts",
        ),
        t(
            "T1021.002",
            "SMB/Windows Admin Shares",
            Tactic::LateralMovement,
            "lateral-smb",
            "plan_only",
            "PLAN_ONLY — never opens SMB sockets",
        ),
        t(
            "T1071",
            "Application Layer Protocol",
            Tactic::CommandAndControl,
            "listen HTTP/DoH/mTLS",
            "live_scoped",
            "aop-2 encrypted beacons; optional rustls mTLS",
        ),
        t(
            "T1071.004",
            "DNS",
            Tactic::CommandAndControl,
            "dns_codec / DoH",
            "live_scoped",
            "Production DNS/DoH C2 codec aop-dns-v1",
        ),
        t(
            "T1090",
            "Proxy",
            Tactic::CommandAndControl,
            "malleable-profile",
            "safe",
            "Malleable HTTP profile shapes C2 traffic (lab)",
        ),
        t(
            "T1041",
            "Exfiltration Over C2 Channel",
            Tactic::Exfiltration,
            "task result / loot",
            "live_scoped",
            "Task results sealed to engagement loot + receipts",
        ),
        t(
            "T1486",
            "Data Encrypted for Impact",
            Tactic::Impact,
            "—",
            "not_claimed",
            "Destructive impact is NOT implemented (authorized red team ethics)",
        ),
        t(
            "T1218",
            "System Binary Proxy Execution (LOLBins)",
            Tactic::DefenseEvasion,
            "lolbas-plan",
            "plan_only",
            "Catalog of living-off-the-land techniques; plan only",
        ),
        t(
            "T1003",
            "OS Credential Dumping",
            Tactic::CredentialAccess,
            "—",
            "not_claimed",
            "Credential dumping is NOT auto-executed (policy)",
        ),
        // ── Credential Access (TA0006) ──
        t(
            "T1110.002",
            "Password Cracking",
            Tactic::CredentialAccess,
            "credential-hash-test",
            "live_scoped",
            "Offline hash cracking against wordlist",
        ),
        t(
            "T1110.003",
            "Password Spraying",
            Tactic::CredentialAccess,
            "credential-spray-plan",
            "plan_only",
            "Credential spray planning — never auto-executes",
        ),
        t(
            "T1552.004",
            "Private Keys",
            Tactic::CredentialAccess,
            "credential-ssh-key-audit",
            "live_scoped",
            "Audit SSH keys for weak permissions and missing passphrases",
        ),
        t(
            "T1552.001",
            "Credentials In Files",
            Tactic::CredentialAccess,
            "credential-env-scan",
            "live_scoped",
            "Scan environment variables for credential patterns",
        ),
        t(
            "T1555.001",
            "Keychain",
            Tactic::CredentialAccess,
            "credential-keychain-plan",
            "plan_only",
            "macOS keychain enumeration planning",
        ),
        // ── Privilege Escalation (TA0004) ──
        t(
            "T1548.001",
            "Setuid and Setgid",
            Tactic::PrivilegeEscalation,
            "privesc-suid-enum",
            "live_scoped",
            "Find SUID/SGID binaries, cross-reference GTFOBins",
        ),
        t(
            "T1548.003",
            "Sudo and Sudo Caching",
            Tactic::PrivilegeEscalation,
            "privesc-sudo-audit",
            "live_scoped",
            "Parse sudo -l for NOPASSWD and wildcard rules",
        ),
        t(
            "T1574.007",
            "Path Interception by PATH",
            Tactic::PrivilegeEscalation,
            "privesc-writable-path",
            "live_scoped",
            "Find writable directories in PATH",
        ),
        t(
            "T1053.003",
            "Cron",
            Tactic::PrivilegeEscalation,
            "privesc-cron-enum",
            "live_scoped",
            "Enumerate cron jobs and writable cron entries",
        ),
        t(
            "T1068",
            "Exploitation for Privilege Escalation",
            Tactic::PrivilegeEscalation,
            "privesc-kernel-plan",
            "plan_only",
            "Kernel exploit planning — VZ guest only",
        ),
        // ── Discovery (TA0007) ──
        t(
            "T1033",
            "System Owner/User Discovery",
            Tactic::Discovery,
            "discovery-system-enum",
            "live_scoped",
            "whoami, id, groups, uname",
        ),
        t(
            "T1016",
            "System Network Configuration Discovery",
            Tactic::Discovery,
            "discovery-network-enum",
            "live_scoped",
            "ifconfig, routing, ARP, DNS",
        ),
        t(
            "T1049",
            "System Network Connections Discovery",
            Tactic::Discovery,
            "discovery-network-enum",
            "live_scoped",
            "netstat listening ports",
        ),
        t(
            "T1057",
            "Process Discovery",
            Tactic::Discovery,
            "discovery-process-enum",
            "live_scoped",
            "Running processes with user context",
        ),
        t(
            "T1005",
            "Data from Local System",
            Tactic::Discovery,
            "discovery-file-discovery",
            "live_scoped",
            "Find sensitive files (keys, configs, databases)",
        ),
        t(
            "T1046",
            "Network Service Discovery",
            Tactic::Discovery,
            "discovery-service-banner",
            "live_scoped",
            "Service banner grabbing on in-scope hosts",
        ),
        t(
            "T1552.005",
            "Cloud Instance Metadata API",
            Tactic::Discovery,
            "discovery-cloud-metadata-plan",
            "plan_only",
            "Cloud metadata service probe planning",
        ),
        t(
            "T1087.002",
            "Domain Account",
            Tactic::Discovery,
            "discovery-ad-enum-plan",
            "plan_only",
            "Active Directory enumeration planning",
        ),
        // ── Collection (TA0009) ──
        t(
            "T1115",
            "Clipboard Data",
            Tactic::Collection,
            "collection-clipboard",
            "live_scoped",
            "Clipboard capture via pbpaste/xclip",
        ),
        t(
            "T1074.001",
            "Local Data Staging",
            Tactic::Collection,
            "collection-stage-files",
            "live_scoped",
            "Stage files to engagement loot directory",
        ),
        t(
            "T1113",
            "Screen Capture",
            Tactic::Collection,
            "collection-screen-plan",
            "plan_only",
            "Screen capture planning — VZ guest only",
        ),
        t(
            "T1056.001",
            "Keylogging",
            Tactic::Collection,
            "collection-keylog-plan",
            "plan_only",
            "Keylogging technique planning — never auto-executes",
        ),
        t(
            "T1560.001",
            "Archive via Utility",
            Tactic::Collection,
            "collection-archive-loot",
            "live_scoped",
            "Archive engagement loot as evidence bundle",
        ),
        // ── Defense Evasion (TA0005) ──
        t(
            "T1518.001",
            "Security Software Discovery",
            Tactic::DefenseEvasion,
            "evasion-security-product-enum",
            "live_scoped",
            "Detect installed AV/EDR products",
        ),
        t(
            "T1070.006",
            "Timestomp",
            Tactic::DefenseEvasion,
            "evasion-timestomp-plan",
            "plan_only",
            "Timestamp manipulation planning",
        ),
        t(
            "T1070.001",
            "Clear Windows Event Logs",
            Tactic::DefenseEvasion,
            "evasion-log-clear-plan",
            "plan_only",
            "Log clearing detection test planning",
        ),
        t(
            "T1562.001",
            "Disable or Modify Tools",
            Tactic::DefenseEvasion,
            "evasion-amsi-bypass-plan",
            "plan_only",
            "AMSI/ETW bypass planning — Windows only",
        ),
        t(
            "T1055.012",
            "Process Hollowing",
            Tactic::DefenseEvasion,
            "evasion-process-hollowing-plan",
            "plan_only",
            "Process hollowing technique planning",
        ),
        t(
            "T1553.002",
            "Code Signing",
            Tactic::DefenseEvasion,
            "evasion-codesign-check",
            "live_scoped",
            "Binary code signature verification check",
        ),
        // ── Exfiltration (TA0010) ──
        t(
            "T1048.003",
            "Exfiltration Over Unencrypted Non-C2 Protocol",
            Tactic::Exfiltration,
            "exfil-dns-encode",
            "live_scoped",
            "DNS exfiltration encoding — no queries sent",
        ),
        t(
            "T1048.002",
            "Exfiltration Over Asymmetric Encrypted Non-C2 Protocol",
            Tactic::Exfiltration,
            "exfil-http-stage",
            "live_scoped",
            "HTTP exfiltration staging manifest — no data transmitted",
        ),
        t(
            "T1572",
            "Protocol Tunneling",
            Tactic::Exfiltration,
            "exfil-tunnel-plan",
            "plan_only",
            "DNS/ICMP/SSH/WebSocket tunneling planning",
        ),
        t(
            "T1027.003",
            "Steganography",
            Tactic::DefenseEvasion,
            "exfil-stego-plan",
            "plan_only",
            "Steganography exfiltration planning",
        ),
        // ── Persistence (TA0003) ──
        t(
            "T1546.004",
            "Unix Shell Configuration Modification",
            Tactic::Persistence,
            "postex-persistence-enum",
            "live_scoped",
            "Shell profile persistence vector enumeration",
        ),
        t(
            "T1543.004",
            "Launch Daemon",
            Tactic::Persistence,
            "postex-persistence-enum",
            "live_scoped",
            "macOS Launch Daemon/Agent persistence enumeration",
        ),
        t(
            "T1543.002",
            "Systemd Service",
            Tactic::Persistence,
            "postex-persistence-enum",
            "live_scoped",
            "Linux systemd service persistence enumeration",
        ),
        // ── Infrastructure / C2 ──
        t(
            "T1219",
            "Remote Access Software",
            Tactic::CommandAndControl,
            "infra-c2-framework-guide",
            "safe",
            "C2 framework comparison and selection guide",
        ),
        t(
            "T1090.002",
            "External Proxy",
            Tactic::CommandAndControl,
            "infra-redirector-plan",
            "plan_only",
            "Redirector architecture planning",
        ),
        t(
            "T1090.004",
            "Domain Fronting",
            Tactic::CommandAndControl,
            "infra-domain-fronting-plan",
            "plan_only",
            "Domain fronting analysis and planning",
        ),
    ]
}

fn t(id: &str, name: &str, tactic: Tactic, surface: &str, mode: &str, notes: &str) -> Technique {
    Technique {
        id: id.into(),
        name: name.into(),
        tactic,
        aop_surface: surface.into(),
        execution_mode: mode.into(),
        notes: notes.into(),
    }
}

pub fn catalog_json() -> serde_json::Value {
    let techs = catalog();
    let mut by_tactic: Vec<serde_json::Value> = Vec::new();
    let mut tactics = [
        Tactic::Reconnaissance,
        Tactic::ResourceDevelopment,
        Tactic::InitialAccess,
        Tactic::Execution,
        Tactic::Persistence,
        Tactic::PrivilegeEscalation,
        Tactic::DefenseEvasion,
        Tactic::CredentialAccess,
        Tactic::Discovery,
        Tactic::LateralMovement,
        Tactic::Collection,
        Tactic::CommandAndControl,
        Tactic::Exfiltration,
        Tactic::Impact,
    ];
    tactics.sort_by_key(|t| t.order());
    for tac in tactics {
        let items: Vec<_> = techs.iter().filter(|x| x.tactic == tac).cloned().collect();
        if items.is_empty() {
            continue;
        }
        by_tactic.push(json!({
            "tactic_id": tac.id(),
            "tactic": tac.name(),
            "order": tac.order(),
            "techniques": items,
        }));
    }
    json!({
        "schema": "aop-attck-v1",
        "framework": "MITRE ATT&CK",
        "technique_count": techs.len(),
        "kill_chain": by_tactic,
        "policy": {
            "authorized_engagements_only": true,
            "destructive_impact_not_implemented": true,
            "credential_dumping_not_auto_executed": true,
            "crash_work_requires_vz": true,
        },
    })
}

fn word_match(haystack: &str, keyword: &str) -> bool {
    for (i, _) in haystack.match_indices(keyword) {
        let before_ok = i == 0 || !haystack.as_bytes()[i - 1].is_ascii_alphanumeric();
        let after = i + keyword.len();
        let after_ok =
            after >= haystack.len() || !haystack.as_bytes()[after].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

/// Map a free-text action/module name to technique IDs.
pub fn map_action(action: &str) -> Vec<&'static str> {
    let a = action.to_ascii_lowercase();
    let mut out = Vec::new();
    if a.contains("recon") || a.contains("scan") {
        out.extend(["T1595", "T1592"]);
    }
    if a.contains("engage") {
        out.push("T1583");
    }
    if a.contains("phish") {
        out.push("T1566");
    }
    if a.contains("exploit") || a.contains("poc") || a.contains("fuzz") {
        out.push("T1203");
    }
    if a.contains("inject") {
        out.push("T1055");
    }
    if a.contains("persist") || a.contains("launchagent") {
        out.push("T1543.001");
    }
    if a.contains("pack") || a.contains("scramble") || a.contains("xor") {
        out.push("T1027");
    }
    if a.contains("lateral") && a.contains("ssh") {
        out.push("T1021.004");
    }
    if a.contains("smb") || a.contains("winrm") {
        out.push("T1021.002");
    }
    if a.contains("listen") || a.contains("beacon") || a.contains("c2") || a.contains("task") {
        out.extend(["T1071", "T1041"]);
    }
    if a.contains("dns") || a.contains("doh") {
        out.push("T1071.004");
    }
    if a.contains("malleable") {
        out.push("T1090");
    }
    if a.contains("lolbas") || a.contains("lolbin") {
        out.push("T1218");
    }
    if a.contains("whoami") || a.contains("uname") || a.contains("hostname") {
        out.extend(["T1082", "T1059"]);
    }
    if word_match(&a, "ls") || word_match(&a, "pwd") || word_match(&a, "cat") {
        out.push("T1083");
    }
    // Credential access
    if a.contains("credential") || a.contains("hash") || a.contains("crack") {
        out.push("T1110.002");
    }
    if a.contains("spray") {
        out.push("T1110.003");
    }
    if a.contains("ssh") && a.contains("key") || a.contains("ssh-key-audit") {
        out.push("T1552.004");
    }
    if a.contains("env") && a.contains("scan") || a.contains("credential-env") {
        out.push("T1552.001");
    }
    if a.contains("keychain") {
        out.push("T1555.001");
    }
    // Privilege escalation
    if a.contains("suid") || a.contains("sgid") {
        out.push("T1548.001");
    }
    if a.contains("sudo") {
        out.push("T1548.003");
    }
    if a.contains("writable") && a.contains("path") {
        out.push("T1574.007");
    }
    if a.contains("cron") && !a.contains("micro") {
        out.push("T1053.003");
    }
    if a.contains("kernel") && a.contains("exploit") || a.contains("privesc-kernel") {
        out.push("T1068");
    }
    // Discovery
    if a.contains("discovery") && (a.contains("system") || a.contains("enum")) {
        out.extend(["T1033", "T1016", "T1049", "T1057"]);
    }
    if a.contains("file-discovery") || a.contains("sensitive") && a.contains("file") {
        out.push("T1005");
    }
    if a.contains("banner") || a.contains("service") && a.contains("discovery") {
        out.push("T1046");
    }
    if a.contains("cloud") && a.contains("metadata") {
        out.push("T1552.005");
    }
    if a.contains("ad-enum") || a.contains("active-directory") {
        out.push("T1087.002");
    }
    // Collection
    if a.contains("clipboard") {
        out.push("T1115");
    }
    if a.contains("stage") && a.contains("file") || a.contains("stage-files") {
        out.push("T1074.001");
    }
    if a.contains("screen") && (a.contains("capture") || a.contains("plan")) {
        out.push("T1113");
    }
    if a.contains("keylog") {
        out.push("T1056.001");
    }
    if a.contains("archive") && a.contains("loot") {
        out.push("T1560.001");
    }
    // Defense evasion (new)
    if a.contains("security") && a.contains("product") || a.contains("edr") && a.contains("enum") {
        out.push("T1518.001");
    }
    if a.contains("timestomp") {
        out.push("T1070.006");
    }
    if a.contains("log") && a.contains("clear") {
        out.extend(["T1070.001"]);
    }
    if a.contains("amsi") || a.contains("etw") {
        out.push("T1562.001");
    }
    if a.contains("hollowing") || a.contains("dll") && a.contains("inject") {
        out.push("T1055.012");
    }
    if a.contains("codesign") || a.contains("code-signing") {
        out.push("T1553.002");
    }
    // Exfiltration
    if a.contains("dns") && a.contains("exfil") || a.contains("dns-encode") {
        out.push("T1048.003");
    }
    if a.contains("http") && a.contains("stage") || a.contains("http-stage") {
        out.push("T1048.002");
    }
    if a.contains("stego") {
        out.push("T1027.003");
    }
    if a.contains("tunnel") {
        out.push("T1572");
    }
    // Persistence (post-ex)
    if a.contains("persistence") && a.contains("enum") {
        out.extend(["T1546.004", "T1543.004", "T1543.002"]);
    }
    if a.contains("launch") && a.contains("agent") || a.contains("launchagent") {
        out.push("T1543.004");
    }
    if a.contains("systemd") {
        out.push("T1543.002");
    }
    // Infrastructure
    if a.contains("redirector") {
        out.push("T1090.002");
    }
    if a.contains("domain") && a.contains("fronting") {
        out.push("T1090.004");
    }
    if a.contains("c2") && a.contains("framework") || a.contains("c2-framework") {
        out.push("T1219");
    }
    out.sort_unstable();
    out.dedup();
    out
}

pub fn map_action_json(action: &str) -> serde_json::Value {
    let ids = map_action(action);
    let techs: Vec<_> = catalog()
        .into_iter()
        .filter(|t| ids.iter().any(|id| *id == t.id))
        .collect();
    json!({
        "action": action,
        "technique_ids": ids,
        "techniques": techs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tactic_ids_are_all_ta_prefixed() {
        for t in [
            Tactic::Reconnaissance,
            Tactic::ResourceDevelopment,
            Tactic::InitialAccess,
            Tactic::Execution,
            Tactic::Persistence,
            Tactic::PrivilegeEscalation,
            Tactic::DefenseEvasion,
            Tactic::CredentialAccess,
            Tactic::Discovery,
            Tactic::LateralMovement,
            Tactic::Collection,
            Tactic::CommandAndControl,
            Tactic::Exfiltration,
            Tactic::Impact,
        ] {
            assert!(t.id().starts_with("TA"), "{:?} id={}", t, t.id());
        }
    }

    #[test]
    fn tactic_order_is_1_through_14_no_gaps() {
        let mut orders: Vec<u8> = [
            Tactic::Reconnaissance,
            Tactic::ResourceDevelopment,
            Tactic::InitialAccess,
            Tactic::Execution,
            Tactic::Persistence,
            Tactic::PrivilegeEscalation,
            Tactic::DefenseEvasion,
            Tactic::CredentialAccess,
            Tactic::Discovery,
            Tactic::LateralMovement,
            Tactic::Collection,
            Tactic::CommandAndControl,
            Tactic::Exfiltration,
            Tactic::Impact,
        ]
        .iter()
        .map(|t| t.order())
        .collect();
        orders.sort();
        orders.dedup();
        assert_eq!(orders, (1..=14).collect::<Vec<u8>>());
    }

    #[test]
    fn catalog_has_minimum_techniques_all_with_required_fields() {
        let cat = catalog();
        assert!(
            cat.len() >= 50,
            "catalog should have >= 50 techniques, got {}",
            cat.len()
        );
        for tech in &cat {
            assert!(!tech.id.is_empty(), "empty id");
            assert!(!tech.name.is_empty(), "empty name");
            assert!(
                !tech.aop_surface.is_empty(),
                "empty aop_surface for {}",
                tech.id
            );
        }
    }

    #[test]
    fn catalog_no_duplicate_ids() {
        let cat = catalog();
        let mut ids: Vec<String> = cat.iter().map(|t| t.id.clone()).collect();
        ids.sort();
        let before = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), before, "duplicate technique IDs");
    }

    #[test]
    fn map_action_recon_scan_returns_expected() {
        let ids = map_action("recon scan");
        assert!(ids.contains(&"T1595"), "missing T1595: {:?}", ids);
        assert!(ids.contains(&"T1592"), "missing T1592: {:?}", ids);
    }

    #[test]
    fn map_action_lateral_ssh_returns_t1021_004() {
        let ids = map_action("lateral ssh");
        assert!(ids.contains(&"T1021.004"), "missing T1021.004: {:?}", ids);
    }

    #[test]
    fn map_action_unknown_returns_empty() {
        let ids = map_action("zzzz-no-match-xyz");
        assert!(ids.is_empty(), "expected empty: {:?}", ids);
    }

    #[test]
    fn catalog_round_trips_through_map_action() {
        for tech in catalog() {
            if tech.execution_mode == "not_claimed" {
                continue;
            }
            let ids = map_action(&tech.aop_surface);
            assert!(
                ids.contains(&tech.id.as_str()),
                "CLAIMS-19: ATT&CK technique {} ({}) with aop_surface {:?} is not \
                 reachable via map_action.  The purple report will show a false \
                 detection gap for an action that was performed.  Either add a \
                 keyword branch in map_action or fix the aop_surface string.  \
                 map_action returned: {:?}",
                tech.id,
                tech.name,
                tech.aop_surface,
                ids,
            );
        }
    }

    #[test]
    fn word_match_rejects_substring_inside_word() {
        assert!(
            !word_match("attck_catalog", "cat"),
            "\"cat\" inside \"catalog\" must not match — false T1083 on documentation module"
        );
        assert!(
            !word_match("mtls", "ls"),
            "\"ls\" inside \"mtls\" must not match — false T1083 on C2 listener"
        );
        assert!(
            !word_match("lolbas_catalog", "cat"),
            "\"cat\" inside \"catalog\" must not match — false T1083 on LOLBins catalog"
        );
    }

    #[test]
    fn word_match_accepts_delimited_keywords() {
        assert!(word_match("ls", "ls"), "bare \"ls\" must match");
        assert!(
            word_match("agent:ls/pwd", "ls"),
            "colon-delimited \"ls\" must match"
        );
        assert!(
            word_match("agent:ls/pwd", "pwd"),
            "slash-delimited \"pwd\" must match"
        );
        assert!(word_match("cat", "cat"), "bare \"cat\" must match");
        assert!(
            word_match("agent:cat", "cat"),
            "colon-delimited \"cat\" must match"
        );
    }

    #[test]
    fn map_action_ls_still_maps_to_t1083() {
        let ids = map_action("ls");
        assert!(
            ids.contains(&"T1083"),
            "bare ls must still reach T1083: {:?}",
            ids
        );
        let ids2 = map_action("agent:ls/pwd");
        assert!(
            ids2.contains(&"T1083"),
            "agent:ls/pwd must still reach T1083: {:?}",
            ids2
        );
    }

    #[test]
    fn map_action_attck_catalog_no_false_t1083() {
        let ids = map_action("attck_catalog");
        assert!(
            !ids.contains(&"T1083"),
            "attck_catalog must NOT map to T1083 — \"cat\" inside \
             \"catalog\" is a substring collision, not file discovery: {:?}",
            ids
        );
    }

    #[test]
    fn map_action_engage_init_maps_to_t1583() {
        let ids = map_action("engage-init");
        assert!(
            ids.contains(&"T1583"),
            "engage-init must reach T1583: {:?}",
            ids
        );
        let ids2 = map_action("engage_init");
        assert!(
            ids2.contains(&"T1583"),
            "engage_init must reach T1583: {:?}",
            ids2
        );
    }
}
