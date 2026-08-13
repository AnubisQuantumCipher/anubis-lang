//! Defense evasion module — T10 (TA0005).
//!
//! Techniques for testing defensive control coverage. Live checks run
//! inside VZ guests. Planning artifacts are host-side.
//! Not production AV evasion — lab detection-gap testing.

use super::engagement::Engagement;
use anyhow::Result;
use serde_json::{json, Value};
use std::path::Path;
use std::process::Command;

/// Detect installed AV/EDR products on the current system.
///
/// Enumerates running security software to map the defensive surface.
/// Maps to T1518.001 (Security Software Discovery).
pub fn security_product_enum(eng: &Engagement) -> Result<Value> {
    eng.validate_live()?;

    let signatures = [
        ("CrowdStrike Falcon", &["falcond", "falcon-sensor", "CSFalconService"][..]),
        ("SentinelOne", &["sentineld", "SentinelAgent"]),
        ("Carbon Black", &["cbagentd", "cbdaemon", "CbDefense"]),
        ("Microsoft Defender", &["MsMpEng", "mdatp", "wdavdaemon"]),
        ("Symantec/Broadcom", &["SymDaemon", "symantec", "sep"]),
        ("Sophos", &["SophosScanD", "sophossxld"]),
        ("ESET", &["esets_daemon", "ekrn"]),
        ("Kaspersky", &["klnagent", "kav"]),
        ("Malwarebytes", &["MalwarebytesMac", "mbamservice"]),
        ("Jamf Protect", &["JamfProtect", "com.jamf.protect"]),
        ("Kandji", &["kandji-daemon"]),
        ("macOS XProtect", &["XProtect", "syspolicyd"]),
        ("macOS Gatekeeper", &["com.apple.security.assessment"]),
        ("Little Snitch", &["at.obdev.LittleSnitchDaemon"]),
        ("BlockBlock", &["BlockBlock"]),
        ("Objective-See tools", &["LuLu", "RansomWhere", "KnockKnock"]),
    ];

    let ps_output = Command::new("ps")
        .args(["aux"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    let mut detected: Vec<Value> = Vec::new();
    let mut not_detected: Vec<&str> = Vec::new();

    for (product, patterns) in &signatures {
        let found = patterns.iter().any(|p| ps_output.contains(p));
        if found {
            let matched: Vec<&&str> = patterns.iter().filter(|p| ps_output.contains(**p)).collect();
            detected.push(json!({
                "product": product,
                "processes_matched": matched,
                "running": true,
            }));
        } else {
            not_detected.push(product);
        }
    }

    // macOS-specific: check TCC database for accessibility/screen recording perms
    let tcc_accessible = Command::new("sqlite3")
        .args([
            &format!("{}/Library/Application Support/com.apple.TCC/TCC.db",
                std::env::var("HOME").unwrap_or_default()),
            "SELECT client FROM access WHERE service='kTCCServiceAccessibility' AND allowed=1;",
        ])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

    Ok(json!({
        "schema": "aop-evasion-v1",
        "module": "security_product_enum",
        "engagement_id": eng.engagement_id,
        "detected": detected,
        "not_detected": not_detected,
        "detection_count": detected.len(),
        "tcc_accessibility": tcc_accessible,
        "attck": ["T1518.001"],
        "executed": true,
        "note": "Detection enumeration for defensive gap analysis — not for evasion",
    }))
}

/// Timestamp manipulation planning (T1070.006).
pub fn timestomp_plan(eng: &Engagement, target_path: &str) -> Result<Value> {
    eng.validate_live()?;
    Ok(json!({
        "schema": "aop-evasion-v1",
        "module": "timestomp_plan",
        "status": "PLAN_ONLY",
        "executed": false,
        "engagement_id": eng.engagement_id,
        "target": target_path,
        "attck": ["T1070.006"],
        "methods": {
            "macos": "touch -t YYYYMMDDhhmm <file>  |  SetFile -d/-m (Xcode tools)",
            "linux": "touch -t / touch -r <reference>",
            "forensic_impact": "mtime/atime/ctime — note ctime cannot be set by touch",
        },
        "detection_question": "Does file integrity monitoring (FIM) detect timestamp anomalies?",
        "policy": {
            "never_auto_executes": true,
            "requires_vz_guest": true,
        },
    }))
}

/// Log clearing detection test — checks if clearing common logs is detected.
pub fn log_clear_plan(eng: &Engagement) -> Result<Value> {
    eng.validate_live()?;
    Ok(json!({
        "schema": "aop-evasion-v1",
        "module": "log_clear_plan",
        "status": "PLAN_ONLY",
        "executed": false,
        "engagement_id": eng.engagement_id,
        "attck": ["T1070.001", "T1070.002"],
        "targets": {
            "macos": [
                "/var/log/system.log",
                "/var/log/asl/*.asl",
                "log show / log stream (Unified Logging)",
                "~/Library/Logs/",
            ],
            "linux": [
                "/var/log/auth.log",
                "/var/log/syslog",
                "/var/log/secure",
                "/var/log/wtmp",
                "/var/log/btmp",
                "journalctl --vacuum-time",
            ],
        },
        "detection_question": "Does SIEM alert on log truncation or rapid log deletion?",
        "policy": {
            "never_auto_executes": true,
            "requires_vz_guest": true,
        },
    }))
}

/// AMSI/ETW bypass planning (Windows, PLAN_ONLY).
pub fn amsi_bypass_plan(eng: &Engagement) -> Result<Value> {
    eng.validate_live()?;
    Ok(json!({
        "schema": "aop-evasion-v1",
        "module": "amsi_bypass_plan",
        "status": "PLAN_ONLY",
        "executed": false,
        "engagement_id": eng.engagement_id,
        "platform": "windows",
        "attck": ["T1562.001"],
        "techniques": [
            {
                "name": "AMSI memory patch",
                "method": "Patch AmsiScanBuffer return value in-memory",
                "detection": "Behavioral detection on amsi.dll memory writes",
            },
            {
                "name": "ETW patch",
                "method": "Patch EtwEventWrite to suppress .NET assembly load events",
                "detection": "Kernel telemetry / integrity monitoring",
            },
            {
                "name": "CLR hooking",
                "method": "Hook .NET runtime to bypass logging",
                "detection": "API hooking detection / stack trace anomaly",
            },
            {
                "name": "Reflection-based loading",
                "method": "Load assemblies via reflection to avoid disk writes",
                "detection": "In-memory scanning / behavioral analytics",
            },
        ],
        "policy": {
            "never_auto_executes": true,
            "windows_only": true,
            "requires_vz_guest": true,
        },
    }))
}

/// Process hollowing / injection technique planning (PLAN_ONLY).
pub fn process_hollowing_plan(eng: &Engagement) -> Result<Value> {
    eng.validate_live()?;
    Ok(json!({
        "schema": "aop-evasion-v1",
        "module": "process_hollowing_plan",
        "status": "PLAN_ONLY",
        "executed": false,
        "engagement_id": eng.engagement_id,
        "attck": ["T1055.012", "T1055.001", "T1055.003"],
        "techniques": [
            {
                "id": "T1055.012",
                "name": "Process Hollowing",
                "method": "Create suspended process → unmap → write shellcode → resume",
                "platform": "windows",
            },
            {
                "id": "T1055.001",
                "name": "DLL Injection",
                "method": "OpenProcess → VirtualAllocEx → WriteProcessMemory → CreateRemoteThread",
                "platform": "windows",
            },
            {
                "id": "T1055.003",
                "name": "Thread Execution Hijacking",
                "method": "SuspendThread → SetThreadContext → ResumeThread",
                "platform": "windows",
            },
            {
                "name": "dylib injection (macOS)",
                "method": "DYLD_INSERT_LIBRARIES or task_for_pid + mach_vm_write",
                "platform": "macos",
                "note": "SIP prevents injection into Apple-signed binaries",
            },
            {
                "name": "ptrace injection (Linux)",
                "method": "ptrace(PTRACE_ATTACH) → write shellcode → ptrace(PTRACE_CONT)",
                "platform": "linux",
            },
        ],
        "detection_question": "Does EDR detect cross-process memory writes and remote thread creation?",
        "policy": {
            "never_auto_executes": true,
            "requires_dual_auth": true,
            "requires_vz_guest": true,
        },
    }))
}

/// Binary signature check — does the target binary have code signing?
pub fn codesign_check(eng: &Engagement, binary_path: &Path) -> Result<Value> {
    eng.validate_live()?;
    if !binary_path.is_file() {
        return Ok(json!({
            "schema": "aop-evasion-v1",
            "module": "codesign_check",
            "engagement_id": eng.engagement_id,
            "path": binary_path.display().to_string(),
            "exists": false,
            "attck": ["T1553.002"],
            "executed": true,
        }));
    }

    let codesign = Command::new("codesign")
        .args(["-dvvv", binary_path.to_str().unwrap_or("")])
        .output();

    let (signed, details) = match codesign {
        Ok(o) => {
            let combined = format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            );
            (o.status.success() || combined.contains("Authority="), combined)
        }
        Err(e) => (false, e.to_string()),
    };

    let hardened = details.contains("runtime");
    let notarized = details.contains("Notarization");

    Ok(json!({
        "schema": "aop-evasion-v1",
        "module": "codesign_check",
        "engagement_id": eng.engagement_id,
        "path": binary_path.display().to_string(),
        "signed": signed,
        "hardened_runtime": hardened,
        "notarized": notarized,
        "details": details.lines().take(20).collect::<Vec<_>>().join("\n"),
        "attck": ["T1553.002"],
        "executed": true,
    }))
}

/// Full evasion assessment — combines all detection-gap checks.
pub fn evasion_assessment(eng: &Engagement) -> Result<Value> {
    eng.validate_live()?;
    let products = security_product_enum(eng)?;
    let timestomp = timestomp_plan(eng, "/tmp/example")?;
    let logs = log_clear_plan(eng)?;
    let amsi = amsi_bypass_plan(eng)?;
    let hollowing = process_hollowing_plan(eng)?;

    let detection_count = products["detection_count"].as_u64().unwrap_or(0);

    Ok(json!({
        "schema": "aop-evasion-v1",
        "module": "evasion_assessment",
        "engagement_id": eng.engagement_id,
        "security_products": products,
        "timestomp_plan": timestomp,
        "log_clear_plan": logs,
        "amsi_bypass_plan": amsi,
        "process_injection_plan": hollowing,
        "security_product_count": detection_count,
        "overall_posture": if detection_count >= 2 { "defended" } else { "minimal_defense" },
        "attck": [
            "T1518.001", "T1070.006", "T1070.001", "T1562.001",
            "T1055.012", "T1553.002"
        ],
        "executed": true,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::offensive::engagement::Engagement;

    #[test]
    fn security_product_enum_runs() {
        let eng = Engagement::default_lab("evasion-test", "lab-auth");
        let result = security_product_enum(&eng).unwrap();
        assert_eq!(result["module"], "security_product_enum");
        assert!(result["detected"].is_array());
        assert_eq!(result["executed"], true);
    }

    #[test]
    fn timestomp_plan_is_plan_only() {
        let eng = Engagement::default_lab("evasion-test", "lab-auth");
        let result = timestomp_plan(&eng, "/tmp/test").unwrap();
        assert_eq!(result["status"], "PLAN_ONLY");
    }

    #[test]
    fn amsi_bypass_is_windows_plan_only() {
        let eng = Engagement::default_lab("evasion-test", "lab-auth");
        let result = amsi_bypass_plan(&eng).unwrap();
        assert_eq!(result["status"], "PLAN_ONLY");
        assert_eq!(result["platform"], "windows");
    }

    #[test]
    fn process_hollowing_is_plan_only() {
        let eng = Engagement::default_lab("evasion-test", "lab-auth");
        let result = process_hollowing_plan(&eng).unwrap();
        assert_eq!(result["status"], "PLAN_ONLY");
    }

    #[test]
    fn codesign_check_handles_missing_file() {
        let eng = Engagement::default_lab("evasion-test", "lab-auth");
        let result = codesign_check(&eng, Path::new("/nonexistent/binary")).unwrap();
        assert_eq!(result["exists"], false);
    }

    #[test]
    fn evasion_assessment_combines_all() {
        let eng = Engagement::default_lab("evasion-test", "lab-auth");
        let result = evasion_assessment(&eng).unwrap();
        assert_eq!(result["module"], "evasion_assessment");
        assert!(result["security_products"].is_object());
    }
}
