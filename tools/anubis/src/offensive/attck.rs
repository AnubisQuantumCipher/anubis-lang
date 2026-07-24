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
            "agent modules (shell-ish lab)",
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

/// Map a free-text action/module name to technique IDs.
pub fn map_action(action: &str) -> Vec<&'static str> {
    let a = action.to_ascii_lowercase();
    let mut out = Vec::new();
    for tech in catalog() {
        if a.contains(&tech.aop_surface.to_ascii_lowercase())
            || tech
                .aop_surface
                .to_ascii_lowercase()
                .split(['/', ' ', ':'])
                .any(|p| !p.is_empty() && a.contains(p))
        {
            // leak static by cloning into owned later — use owned ids
            let _ = tech.id;
        }
    }
    // concrete mapping table for reliability
    if a.contains("recon") || a.contains("scan") {
        out.extend(["T1595", "T1592"]);
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
    if a.contains("ls") || a.contains("pwd") || a.contains("cat") {
        out.push("T1083");
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
