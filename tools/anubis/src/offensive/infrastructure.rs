//! Infrastructure module — engagement-scoped red-team infrastructure.
//!
//! C2 listener planning, redirector setup, domain fronting analysis,
//! and infrastructure health checks. All live operations confined to
//! VZ guests or localhost.

use super::engagement::Engagement;
use anyhow::Result;
use serde_json::{json, Value};
use std::net::TcpListener;

/// C2 listener setup on localhost (lab testing only).
///
/// Binds a TCP listener to verify port availability and test
/// C2 framework connectivity. Maps to T1071.001.
pub fn c2_listener_check(eng: &Engagement, port: u16) -> Result<Value> {
    eng.validate_live()?;
    let addr = format!("127.0.0.1:{port}");
    let available = TcpListener::bind(&addr).is_ok();

    Ok(json!({
        "schema": "aop-infra-v1",
        "module": "c2_listener_check",
        "engagement_id": eng.engagement_id,
        "address": addr,
        "port": port,
        "port_available": available,
        "attck": ["T1071.001"],
        "executed": true,
        "note": "Port availability check only — no persistent listener started",
    }))
}

/// C2 framework comparison and selection guide.
pub fn c2_framework_guide(eng: &Engagement) -> Result<Value> {
    eng.validate_live()?;
    Ok(json!({
        "schema": "aop-infra-v1",
        "module": "c2_framework_guide",
        "engagement_id": eng.engagement_id,
        "attck": ["T1219"],
        "frameworks": [
            {
                "name": "Cobalt Strike",
                "type": "commercial",
                "protocols": ["HTTPS", "DNS", "SMB", "TCP"],
                "edr_detection": "high (well-signatured)",
                "malleable_c2": true,
                "opsec": "Requires malleable profiles + artifact kit to evade",
            },
            {
                "name": "Sliver",
                "type": "open_source",
                "protocols": ["mTLS", "WireGuard", "HTTPS", "DNS"],
                "edr_detection": "medium",
                "malleable_c2": false,
                "opsec": "Compiled implants, good OPSEC defaults",
            },
            {
                "name": "Havoc",
                "type": "open_source",
                "protocols": ["HTTPS", "SMB"],
                "edr_detection": "low-medium (newer)",
                "malleable_c2": true,
                "opsec": "Indirect syscalls, sleep obfuscation",
            },
            {
                "name": "Mythic",
                "type": "open_source",
                "protocols": ["HTTP", "TCP", "SMB", "WebSocket"],
                "edr_detection": "medium",
                "malleable_c2": false,
                "opsec": "Multi-agent, operator-friendly, extensible",
            },
            {
                "name": "Brute Ratel C4",
                "type": "commercial",
                "protocols": ["HTTPS", "DNS", "SMB", "DoH"],
                "edr_detection": "low (designed for AV/EDR evasion)",
                "malleable_c2": true,
                "opsec": "Syscall evasion, ETW patching, sleep masking",
            },
        ],
        "selection_criteria": [
            "Target defensive stack (AV/EDR product)",
            "Network egress restrictions (proxies, DPI)",
            "Required protocols (DNS-only? HTTPS-only?)",
            "Operator skill level and engagement timeline",
            "OPSEC requirements and attribution concerns",
        ],
        "executed": true,
    }))
}

/// Redirector setup planning (PLAN_ONLY).
pub fn redirector_plan(eng: &Engagement) -> Result<Value> {
    eng.validate_live()?;
    Ok(json!({
        "schema": "aop-infra-v1",
        "module": "redirector_plan",
        "status": "PLAN_ONLY",
        "executed": false,
        "engagement_id": eng.engagement_id,
        "attck": ["T1090.002"],
        "architecture": {
            "tiers": [
                {
                    "tier": 1,
                    "role": "Victim-facing redirector",
                    "implementation": "socat / iptables DNAT / Apache mod_rewrite",
                    "disposable": true,
                },
                {
                    "tier": 2,
                    "role": "Team server (C2)",
                    "implementation": "Cobalt Strike / Sliver / Havoc",
                    "disposable": false,
                },
                {
                    "tier": 3,
                    "role": "Long-haul redirector (backup C2)",
                    "implementation": "Domain fronting / CDN / legitimate SaaS",
                    "disposable": false,
                },
            ],
            "domain_categorization": [
                "Age domain 30+ days before engagement",
                "Categorize via web crawlers (health/business/tech)",
                "Match target industry vertical",
                "Use expired domains with existing reputation",
            ],
        },
        "detection_question": "Does blue team inspect SNI/Host mismatch or CDN abuse patterns?",
        "policy": {
            "never_auto_executes": true,
            "requires_vz_guest": true,
        },
    }))
}

/// Domain fronting analysis (PLAN_ONLY).
pub fn domain_fronting_plan(eng: &Engagement) -> Result<Value> {
    eng.validate_live()?;
    Ok(json!({
        "schema": "aop-infra-v1",
        "module": "domain_fronting_plan",
        "status": "PLAN_ONLY",
        "executed": false,
        "engagement_id": eng.engagement_id,
        "attck": ["T1090.004"],
        "technique": {
            "description": "SNI header shows legitimate domain; Host header routes to C2",
            "cdns_historically_vulnerable": [
                "CloudFront (partially blocked since 2018)",
                "Azure CDN (blocked for most cases)",
                "Fastly (variable enforcement)",
                "Cloudflare (blocked)",
            ],
            "modern_alternatives": [
                "Domain borrowing (shared-hosting IP reuse)",
                "CDN domain category abuse (e.g., *.azureedge.net)",
                "Legitimate SaaS API tunneling (Slack, Teams webhooks)",
            ],
        },
        "detection": [
            "SNI vs Host header mismatch inspection",
            "JA3/JA3S TLS fingerprinting",
            "Unusual CDN traffic patterns",
            "DNS analytics (volume, entropy, timing)",
        ],
        "policy": {
            "never_auto_executes": true,
            "requires_vz_guest": true,
        },
    }))
}

/// Infrastructure health check — validate engagement infra.
pub fn infra_health(eng: &Engagement, c2_ports: &[u16]) -> Result<Value> {
    eng.validate_live()?;
    let mut port_status: Vec<Value> = Vec::new();

    for &port in c2_ports {
        let addr = format!("127.0.0.1:{port}");
        let available = TcpListener::bind(&addr).is_ok();
        port_status.push(json!({
            "port": port,
            "available": available,
        }));
    }

    let ports_ready = port_status
        .iter()
        .filter(|p| p["available"] == true)
        .count();

    Ok(json!({
        "schema": "aop-infra-v1",
        "module": "infra_health",
        "engagement_id": eng.engagement_id,
        "port_status": port_status,
        "ports_checked": c2_ports.len(),
        "ports_available": ports_ready,
        "all_ready": ports_ready == c2_ports.len(),
        "attck": ["T1071.001"],
        "executed": true,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::offensive::engagement::Engagement;

    #[test]
    fn c2_listener_check_runs() {
        let eng = Engagement::default_lab("infra-test", "lab-auth");
        let result = c2_listener_check(&eng, 44444).unwrap();
        assert_eq!(result["module"], "c2_listener_check");
        assert_eq!(result["executed"], true);
    }

    #[test]
    fn c2_framework_guide_returns_frameworks() {
        let eng = Engagement::default_lab("infra-test", "lab-auth");
        let result = c2_framework_guide(&eng).unwrap();
        assert!(result["frameworks"].is_array());
        assert!(result["frameworks"].as_array().unwrap().len() >= 4);
    }

    #[test]
    fn redirector_plan_is_plan_only() {
        let eng = Engagement::default_lab("infra-test", "lab-auth");
        let result = redirector_plan(&eng).unwrap();
        assert_eq!(result["status"], "PLAN_ONLY");
    }

    #[test]
    fn domain_fronting_plan_is_plan_only() {
        let eng = Engagement::default_lab("infra-test", "lab-auth");
        let result = domain_fronting_plan(&eng).unwrap();
        assert_eq!(result["status"], "PLAN_ONLY");
    }

    #[test]
    fn infra_health_checks_ports() {
        let eng = Engagement::default_lab("infra-test", "lab-auth");
        let result = infra_health(&eng, &[55555, 55556]).unwrap();
        assert_eq!(result["ports_checked"], 2);
    }
}
