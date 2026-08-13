//! Living-off-the-land (LOLBAS / GTFOBins-style) technique catalog — PLAN_ONLY.
//! Elite red teams prefer trusted binaries; AOP documents lab plans without executing them.

use serde_json::{json, Value};

pub fn catalog_json() -> Value {
    json!({
        "schema": "aop-lolbas-v1",
        "status": "PLAN_ONLY",
        "executed": false,
        "attck": ["T1218", "T1218.011", "T1059"],
        "note": "Catalog for authorized lab planning. AOP does not auto-chain LOLBins against hosts.",
        "macos": [
            {
                "binary": "osascript",
                "use": "AppleScript execution / dialogs",
                "lab_plan": "Document detection for osascript network+exec chains; do not run hostile scripts on host"
            },
            {
                "binary": "curl",
                "use": "Fetch payload over HTTP(S)",
                "lab_plan": "Scope to loopback C2 only; pair with aop-2 agent generate instead of raw curl implants"
            },
            {
                "binary": "launchctl",
                "use": "Load LaunchAgents",
                "lab_plan": "Use persist-launchagent artifact + human install script"
            },
            {
                "binary": "ditto",
                "use": "Archive/copy payloads",
                "lab_plan": "Stage only under engagement loot/"
            },
            {
                "binary": "python3",
                "use": "Inline post-ex scripts",
                "lab_plan": "Prefer agent task modules; VZ for any crash-prone script"
            }
        ],
        "linux": [
            {
                "binary": "bash",
                "use": "Shell execution",
                "lab_plan": "lateral-ssh scoped commands only"
            },
            {
                "binary": "curl|wget",
                "use": "Ingress tool transfer",
                "lab_plan": "Deny egress by default (network_egress=false)"
            },
            {
                "binary": "ssh",
                "use": "Lateral movement",
                "lab_plan": "lateral-ssh + allowed_lateral_hosts fail-closed"
            }
        ],
        "windows_plan_only": [
            {
                "binary": "powershell.exe",
                "use": "Download cradle / in-memory",
                "lab_plan": "Windows tranche PLAN_ONLY — no auto WinRM exec"
            },
            {
                "binary": "rundll32.exe",
                "use": "Proxy execution",
                "lab_plan": "Document EDR rules; no host execution from AOP"
            },
            {
                "binary": "wmic.exe",
                "use": "Remote process create",
                "lab_plan": "Replaced in modern Windows; plan detection only"
            },
            {
                "binary": "certutil.exe",
                "use": "Decode / download",
                "lab_plan": "Classic LOLBin — blue should alert"
            }
        ],
        "aop_commands": [
            "anubis lolbas-catalog --json",
            "anubis purple-report — maps T1218 gaps for blue",
            "anubis campaign-init — phase 5 references LOLBins under defense evasion"
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_plan_only() {
        let c = catalog_json();
        assert_eq!(c["status"], "PLAN_ONLY");
        assert_eq!(c["executed"], false);
    }

    #[test]
    fn catalog_has_correct_attck_refs() {
        let c = catalog_json();
        let attck = c["attck"].as_array().unwrap();
        let ids: Vec<&str> = attck.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(ids.contains(&"T1218"));
        assert!(ids.contains(&"T1059"));
    }

    #[test]
    fn catalog_has_expected_os_counts() {
        let c = catalog_json();
        assert_eq!(c["macos"].as_array().unwrap().len(), 5);
        assert_eq!(c["linux"].as_array().unwrap().len(), 3);
        assert_eq!(c["windows_plan_only"].as_array().unwrap().len(), 4);
    }
}
