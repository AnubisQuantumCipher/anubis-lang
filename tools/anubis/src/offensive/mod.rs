//! Anubis Offensive Platform (AOP) — engagement-scoped red-team / exploit platform.
//!
//! T1 encrypt+jitter+keys+mtls certs | T2 persistence | T3 dns/uds |
//! T4 lateral | T5 rop/browser | T6 packer | T7 console+RBAC

pub mod agent;
pub mod console;
pub mod crypto;
pub mod engagement;
pub mod exploit;
pub mod lateral;
pub mod listener;
pub mod modules;
pub mod packer;
pub mod persistence;
pub mod protocol;
pub mod rop;
pub mod scope;

pub use engagement::{engage_init, engage_status, load_engagement};
