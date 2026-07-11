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
            description: "Process inject PLAN ONLY — research-gated (T2)",
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
            description: "Execute command inside VZ guest — crash + network isolated (T8)",
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
