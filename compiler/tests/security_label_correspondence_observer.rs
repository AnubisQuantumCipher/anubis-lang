//! Completion Blueprint Phase 8, Slice 1 — production-linked correspondence observer,
//! externally observable side.
//!
//! An INTEGRATION test can only reach `pub` items on the compiler crate, so it exercises
//! the same `observe_security_label_correspondence` entry point the correspondence gate
//! (`scripts/run_security_label_correspondence_gate.sh`) drives. The internal unit test
//! `observer_emits_declared_row_count` in `compiler/src/middle/security_label.rs` locks the
//! same guarantees on the crate's private surface; both must hold at all times.
//!
//! When invoked with `ANUBIS_SECURITY_LABEL_OBSERVATIONS_OUT=<path>` in the environment,
//! this test additionally writes the observed corpus to `<path>`. That is how the gate
//! script obtains the Rust-side observation stream: no separate binary, no extra Cargo
//! target, no production-surface widening beyond the two `pub` items in `lib.rs`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::BufWriter;

use anubis_compiler::{
    observe_security_label_correspondence, OBSERVE_SECURITY_LABEL_CORRESPONDENCE_ROW_COUNT,
};

const EMIT_ENV: &str = "ANUBIS_SECURITY_LABEL_OBSERVATIONS_OUT";

fn collect_observations() -> String {
    let mut buf: Vec<u8> = Vec::new();
    observe_security_label_correspondence(&mut buf)
        .expect("observe_security_label_correspondence must not fail on Vec<u8>");
    String::from_utf8(buf).expect("observer output must be valid UTF-8")
}

#[test]
fn public_observer_emits_declared_row_count() {
    let body = collect_observations();
    let rows: Vec<&str> = body.lines().collect();
    assert_eq!(
        rows.len(),
        OBSERVE_SECURITY_LABEL_CORRESPONDENCE_ROW_COUNT,
        "observer row count must equal OBSERVE_SECURITY_LABEL_CORRESPONDENCE_ROW_COUNT ({})",
        OBSERVE_SECURITY_LABEL_CORRESPONDENCE_ROW_COUNT
    );
}

#[test]
fn public_observer_row_shape_is_tsv_with_op_prefix() {
    let body = collect_observations();
    let allowed_ops: BTreeSet<&str> = [
        "from_legacy_taint",
        "from_legacy_secret",
        "join",
        "declassified_by",
        "to_legacy_taint",
        "to_legacy_secret",
    ]
    .into_iter()
    .collect();
    let mut per_op = BTreeMap::<String, usize>::new();
    for row in body.lines() {
        let cols: Vec<&str> = row.splitn(4, '\t').collect();
        assert_eq!(
            cols.len(),
            4,
            "each row must have four tab-separated fields; offending row=`{}`",
            row
        );
        assert!(
            allowed_ops.contains(cols[0]),
            "op `{}` is not in the declared abstraction — did the corpus quietly grow?",
            cols[0]
        );
        *per_op.entry(cols[0].to_string()).or_insert(0) += 1;
    }
    assert_eq!(per_op["from_legacy_taint"], 4);
    assert_eq!(per_op["from_legacy_secret"], 2);
    assert_eq!(per_op["join"], 49);
    assert_eq!(per_op["declassified_by"], 14);
    assert_eq!(per_op["to_legacy_taint"], 7);
    assert_eq!(per_op["to_legacy_secret"], 7);
}

#[test]
fn public_observer_keys_are_unique() {
    let body = collect_observations();
    let mut keys = BTreeSet::<String>::new();
    for row in body.lines() {
        let cols: Vec<&str> = row.splitn(4, '\t').collect();
        let key = format!("{}|{}|{}", cols[0], cols[1], cols[2]);
        assert!(
            keys.insert(key.clone()),
            "duplicate (op,arg1,arg2) key `{}` — the abstract corpus must be a set",
            key
        );
    }
}

#[test]
fn public_observer_emit_to_env_path() {
    // Optional emit-to-file path. When invoked without the env var this test still
    // guards `collect_observations()` so the round-trip is exercised on every `cargo
    // test` run. When invoked WITH the env var, it additionally writes the observed
    // corpus for the correspondence gate to consume.
    let body = collect_observations();
    if let Ok(path) = std::env::var(EMIT_ENV) {
        // Never overwrite in place — use a per-PID temp file first and rename atomically.
        let target = std::path::PathBuf::from(&path);
        let parent = target.parent().expect("emit path must live in a directory");
        assert!(
            parent.exists(),
            "emit path parent directory `{}` must already exist",
            parent.display()
        );
        let tmp = parent.join(format!(
            "{}.rust.tmp.{}",
            target
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("observations"),
            std::process::id()
        ));
        {
            let file = File::create(&tmp).unwrap_or_else(|e| {
                panic!("failed to create tmp emit file at {}: {}", tmp.display(), e)
            });
            let mut writer = BufWriter::new(file);
            std::io::Write::write_all(&mut writer, body.as_bytes()).unwrap_or_else(|e| {
                panic!("failed to write tmp emit file at {}: {}", tmp.display(), e)
            });
        }
        std::fs::rename(&tmp, &target).unwrap_or_else(|e| {
            panic!(
                "failed to rename {} to {}: {}",
                tmp.display(),
                target.display(),
                e
            )
        });
    }
}
