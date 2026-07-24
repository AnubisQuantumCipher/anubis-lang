//! Anubis Offensive Platform (AOP) — engagement-scoped red-team / exploit platform.
//!
//! **Isolation (non-negotiable):** every red-team *execution* path requires an
//! Apple Virtualization guest (`isolation::require_vz_offensive`). The host is
//! control-plane only (planning, catalogs, evidence verify of guest loot).
//!
//! T1 encrypt+jitter+keys+mtls | T2 persistence/inject | T3 dns/doh/uds |
//! T4 lateral | T5 rop/browser | T6 packer | T7 console+RBAC+tokens |
//! T8 VZ sandbox | T9 ATT&CK/OPSEC/campaign/purple/recon/malleable/phish/lolbas

pub mod agent;
pub mod attck;
pub mod campaign;
pub mod console;
pub mod crypto;
pub mod dns_codec;
pub mod engagement;
pub mod exploit;
pub mod isolation;
pub mod lateral;
pub mod listener;
pub mod lolbas;
pub mod malleable;
pub mod modules;
pub mod opsec;
pub mod packer;
pub mod persistence;
pub mod phish;
pub mod protocol;
pub mod purple;
pub mod receipts;
pub mod recon;
pub mod rop;
pub mod scope;
pub mod vz;

pub use engagement::{
    engage_init, engage_status, load_engagement, operator_token_issue, operator_token_revoke,
};
pub use isolation::{in_vz_guest, require_vz_offensive};
pub use receipts::{seal_action, verify_chain};
