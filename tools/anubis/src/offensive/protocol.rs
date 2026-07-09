//! Anubis C2 wire protocol.
//! - aop-1: cleartext JSON (legacy lab)
//! - aop-2: AES-256-GCM envelope over JSON (default)

use serde::{Deserialize, Serialize};

pub const PROTOCOL_V1: &str = "aop-1";
pub const PROTOCOL_V2: &str = "aop-2";
pub const PROTOCOL_VERSION: &str = PROTOCOL_V2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Beacon {
    pub protocol: String,
    pub agent_id: String,
    pub engagement_id: String,
    pub hostname: String,
    pub os: String,
    pub arch: String,
    pub pid: u32,
    pub sleep_ms: u64,
    #[serde(default)]
    pub jitter_pct: u8,
    /// Agent-side key id (hash of agent secret).
    #[serde(default)]
    pub key_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub module: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeaconResponse {
    pub protocol: String,
    pub tasks: Vec<Task>,
    pub sleep_ms: u64,
    #[serde(default)]
    pub jitter_pct: u8,
    #[serde(default)]
    pub die: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub protocol: String,
    pub agent_id: String,
    pub engagement_id: String,
    pub task_id: String,
    pub module: String,
    pub ok: bool,
    pub output: String,
}

/// Wire envelope for aop-2.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedEnvelope {
    pub protocol: String,
    pub engagement_id: String,
    pub agent_id: String,
    /// base64(nonce||ciphertext) of inner JSON
    pub blob: String,
}
