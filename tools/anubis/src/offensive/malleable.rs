//! Malleable C2 HTTP profile — shapes beacon traffic like elite CS/Sliver profiles.
//! Profiles are engagement-scoped JSON; listener can load them for header/URI cosmetics.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MalleableProfile {
    pub schema_version: String,
    pub name: String,
    /// URI paths for beacon POST (rotated).
    pub beacon_uris: Vec<String>,
    pub result_uris: Vec<String>,
    pub user_agent: String,
    /// Extra request headers name→value.
    #[serde(default)]
    pub headers: Vec<[String; 2]>,
    /// Server response header cosmetics.
    #[serde(default)]
    pub server_headers: Vec<[String; 2]>,
    /// Sleep metadata advertised (ms) — actual sleep still from engagement.
    #[serde(default)]
    pub sleep_hint_ms: u64,
    /// Transform: none | base64 | prepend_junk (lab-only labels).
    #[serde(default = "default_transform")]
    pub transform: String,
}

/// Every transform name the profile schema recognises. `apply_transform` must have a
/// non-identity arm for each of these except `none`; the parity test enforces that, so a
/// name added here without an implementation fails the suite instead of silently shaping
/// nothing.
pub const KNOWN_TRANSFORMS: &[&str] = &["none", "base64", "prepend_junk"];

/// The subset of `KNOWN_TRANSFORMS` the BEACON can reverse.
///
/// Deliberately just `none` today. `base64` and `prepend_junk` exist in the listener and are
/// unit-tested, but nothing in the generated agent undoes them — the agent's own base64
/// (`agent.rs` B64 encode/decode) is the crypto envelope, a different layer. Until the agent
/// template learns the inverse, `validate()` refuses those profiles rather than letting an
/// operator enable a transform that quietly breaks their C2.
pub const TRANSFORMS_WITH_BEACON_INVERSE: &[&str] = &["none"];
pub const SUPPORTED_SCHEMA_VERSION: &str = "1.0";
const MAX_PROFILE_BYTES: usize = 1024 * 1024;
const MAX_PROFILE_NAME_BYTES: usize = 128;
const MAX_URIS_PER_DIRECTION: usize = 32;
const MAX_URI_BYTES: usize = 2048;
const MAX_HEADERS_PER_DIRECTION: usize = 64;
const MAX_HEADER_NAME_BYTES: usize = 128;
const MAX_HEADER_VALUE_BYTES: usize = 4096;
const MAX_TOTAL_HEADER_BYTES: usize = 64 * 1024;

fn default_transform() -> String {
    "none".into()
}

fn valid_header_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_HEADER_NAME_BYTES
        && name.bytes().all(|b| {
            b.is_ascii_alphanumeric()
                || matches!(
                    b,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        })
}

fn valid_header_value(value: &str) -> bool {
    value.len() <= MAX_HEADER_VALUE_BYTES
        && value
            .bytes()
            .all(|b| b == b'\t' || (b >= 0x20 && b != 0x7f))
}

impl Default for MalleableProfile {
    fn default() -> Self {
        Self {
            schema_version: "1.0".into(),
            name: "aop-default-jquery".into(),
            beacon_uris: vec!["/jquery-3.6.0.min.js".into(), "/api/v1/telemetry".into()],
            result_uris: vec!["/api/v1/events".into()],
            user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36".into(),
            headers: vec![
                ["Accept".into(), "*/*".into()],
                ["Accept-Language".into(), "en-US,en;q=0.9".into()],
            ],
            server_headers: vec![
                ["Server".into(), "nginx".into()],
                ["X-Content-Type-Options".into(), "nosniff".into()],
            ],
            sleep_hint_ms: 2000,
            transform: "none".into(),
        }
    }
}

impl MalleableProfile {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != SUPPORTED_SCHEMA_VERSION {
            return Err(anyhow!(
                "ANUBIS_MALLEABLE_SCHEMA_VERSION: unsupported `{}` (supported: {})",
                self.schema_version,
                SUPPORTED_SCHEMA_VERSION
            ));
        }
        if self.name.trim().is_empty()
            || self.name.len() > MAX_PROFILE_NAME_BYTES
            || self.name.bytes().any(|b| b.is_ascii_control())
        {
            return Err(anyhow!("ANUBIS_MALLEABLE_NAME"));
        }
        // The transform is applied to every beacon response (listener.rs encode_response).
        // Two ways that goes wrong, both closed here rather than at the call site:
        //
        //   1. An unrecognised name falls through apply_transform's `_` arm to identity, so a
        //      typo (`"base64 "`) or an unimplemented idea (`"xor"`) silently shapes nothing
        //      while the operator believes the profile is live. Fail closed on the DECLARATION
        //      surface, the same way unknown attributes were closed in ec65724.
        //   2. A transform the BEACON cannot reverse mangles every response with nothing on the
        //      other end to undo it. That is worse than the dead field this replaced: inert is
        //      harmless, one-directional is a silent C2 break that only appears once someone
        //      writes a non-default profile.
        if !KNOWN_TRANSFORMS.contains(&self.transform.as_str()) {
            return Err(anyhow!(
                "ANUBIS_MALLEABLE_TRANSFORM_UNKNOWN: `{}` (known: {})",
                self.transform,
                KNOWN_TRANSFORMS.join(", ")
            ));
        }
        if !TRANSFORMS_WITH_BEACON_INVERSE.contains(&self.transform.as_str()) {
            return Err(anyhow!(
                "ANUBIS_MALLEABLE_TRANSFORM_NO_INVERSE: `{}` is implemented in the listener but \
                 the beacon has no inverse for it, so every response would be transformed with \
                 nothing to undo it. Implement the inverse in the agent template's response path \
                 and add the name to TRANSFORMS_WITH_BEACON_INVERSE.",
                self.transform
            ));
        }
        if self.beacon_uris.is_empty() {
            return Err(anyhow!("ANUBIS_MALLEABLE_NO_BEACON_URI"));
        }
        if self.beacon_uris.len() > MAX_URIS_PER_DIRECTION
            || self.result_uris.len() > MAX_URIS_PER_DIRECTION
        {
            return Err(anyhow!(
                "ANUBIS_MALLEABLE_URI_COUNT: beacon={} result={} max_each={MAX_URIS_PER_DIRECTION}",
                self.beacon_uris.len(),
                self.result_uris.len()
            ));
        }
        let mut seen_uris = BTreeSet::new();
        for u in self.beacon_uris.iter().chain(self.result_uris.iter()) {
            if u.len() > MAX_URI_BYTES {
                return Err(anyhow!(
                    "ANUBIS_MALLEABLE_URI_LONG: {} bytes (max {MAX_URI_BYTES})",
                    u.len()
                ));
            }
            if !u.starts_with('/') {
                return Err(anyhow!("ANUBIS_MALLEABLE_URI_MUST_ABS: {u}"));
            }
            // `//host/path` is a PROTOCOL-RELATIVE URL. It starts with `/` so the absolute-path
            // check above passes it, and it contains no `://` so the scheme check below passed it
            // too — a beacon URI that silently resolves to an arbitrary EXTERNAL host, which is the
            // exact thing this validator exists to prevent. Backslash is rejected with it: some
            // clients normalise `\\` to `/`, so `\\evil.com` is the same hole spelled differently.
            if u.starts_with("//") || u.starts_with("/\\") || u.contains('\\') {
                return Err(anyhow!("ANUBIS_MALLEABLE_URI_HOSTILE: {u}"));
            }
            if u.contains("://")
                || u.bytes()
                    .any(|b| b.is_ascii_control() || b.is_ascii_whitespace())
                || u.split('?')
                    .next()
                    .unwrap_or(u)
                    .split('/')
                    .any(|part| part == "." || part == "..")
            {
                return Err(anyhow!("ANUBIS_MALLEABLE_URI_HOSTILE: {u}"));
            }
            if !seen_uris.insert(u) {
                return Err(anyhow!("ANUBIS_MALLEABLE_URI_DUPLICATE: {u}"));
            }
        }
        if self.user_agent.len() > 512 || self.user_agent.bytes().any(|b| b.is_ascii_control()) {
            return Err(anyhow!("ANUBIS_MALLEABLE_UA_LONG"));
        }
        if self.headers.len() > MAX_HEADERS_PER_DIRECTION
            || self.server_headers.len() > MAX_HEADERS_PER_DIRECTION
        {
            return Err(anyhow!(
                "ANUBIS_MALLEABLE_HEADER_COUNT: request={} server={} max_each={MAX_HEADERS_PER_DIRECTION}",
                self.headers.len(),
                self.server_headers.len()
            ));
        }
        let mut total_header_bytes = 0usize;
        for [name, value] in self.headers.iter().chain(self.server_headers.iter()) {
            if !valid_header_name(name) {
                return Err(anyhow!("ANUBIS_MALLEABLE_HEADER_NAME: {name:?}"));
            }
            if !valid_header_value(value) {
                return Err(anyhow!(
                    "ANUBIS_MALLEABLE_HEADER_VALUE: invalid or oversized value for {name:?}"
                ));
            }
            total_header_bytes = total_header_bytes
                .checked_add(name.len() + value.len())
                .ok_or_else(|| anyhow!("ANUBIS_MALLEABLE_HEADER_TOTAL"))?;
        }
        if total_header_bytes > MAX_TOTAL_HEADER_BYTES {
            return Err(anyhow!(
                "ANUBIS_MALLEABLE_HEADER_TOTAL: {total_header_bytes} bytes (max {MAX_TOTAL_HEADER_BYTES})"
            ));
        }
        Ok(())
    }

    pub fn apply_transform(&self, data: &[u8]) -> Result<Vec<u8>> {
        match self.transform.as_str() {
            "none" => Ok(data.to_vec()),
            "base64" => {
                use base64::Engine;
                Ok(base64::engine::general_purpose::STANDARD
                    .encode(data)
                    .into_bytes())
            }
            "prepend_junk" => {
                let junk = b"<!-- cached -->\n";
                let mut out = Vec::with_capacity(junk.len() + data.len());
                out.extend_from_slice(junk);
                out.extend_from_slice(data);
                Ok(out)
            }
            other => Err(anyhow!(
                "ANUBIS_MALLEABLE_TRANSFORM_UNKNOWN: `{other}` (known: {})",
                KNOWN_TRANSFORMS.join(", ")
            )),
        }
    }

    pub fn format_server_headers(&self) -> String {
        let mut out = String::new();
        for pair in &self.server_headers {
            out.push_str(&pair[0]);
            out.push_str(": ");
            out.push_str(&pair[1]);
            out.push_str("\r\n");
        }
        out
    }
}

fn profile_paths(dir: &Path) -> Result<Vec<std::path::PathBuf>> {
    let mut paths = Vec::new();
    let entries =
        fs::read_dir(dir).map_err(|e| anyhow!("ANUBIS_MALLEABLE_DIR: {}: {e}", dir.display()))?;
    for entry in entries {
        let entry =
            entry.map_err(|e| anyhow!("ANUBIS_MALLEABLE_DIR_ENTRY: {}: {e}", dir.display()))?;
        let path = entry.path();
        if !path.extension().is_some_and(|ext| ext == "json") {
            continue;
        }
        let file_type = entry
            .file_type()
            .map_err(|e| anyhow!("ANUBIS_MALLEABLE_DIR_ENTRY_TYPE: {}: {e}", path.display()))?;
        if !file_type.is_file() {
            return Err(anyhow!(
                "ANUBIS_MALLEABLE_PROFILE_NOT_REGULAR: {}",
                path.display()
            ));
        }
        paths.push(path);
    }
    paths.sort();
    Ok(paths)
}

/// Load the engagement's malleable profile, if it has one.
///
/// `Ok(None)` means the operator wrote no profile — the listener runs with its built-in
/// behaviour, which is the honest default. An INVALID profile is an `Err`, not a `None`:
/// this previously ended in `.ok()`, so a profile that failed validation was silently
/// discarded and the listener came up unprofiled while the operator believed their
/// traffic shaping was live. A rejected input that the consumer treats as "absent" is the
/// same producer/consumer split this module just closed one layer down.
pub fn load_from_engage(engage_dir: &Path) -> Result<Option<MalleableProfile>> {
    let dir = engage_dir.join("profiles");
    let metadata = match fs::symlink_metadata(&dir) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(anyhow!(
                "ANUBIS_MALLEABLE_DIR_METADATA: {}: {error}",
                dir.display()
            ));
        }
    };
    if metadata.file_type().is_symlink() {
        return Err(anyhow!("ANUBIS_MALLEABLE_DIR_SYMLINK: {}", dir.display()));
    }
    if !metadata.is_dir() {
        return Err(anyhow!(
            "ANUBIS_MALLEABLE_DIR_NOT_DIRECTORY: {}",
            dir.display()
        ));
    }
    let paths = profile_paths(&dir)?;
    match paths.as_slice() {
        [] => Ok(None),
        [path] => load(path).map(Some),
        _ => Err(anyhow!(
            "ANUBIS_MALLEABLE_AMBIGUOUS: {} JSON profiles in {}; exactly one is allowed",
            paths.len(),
            dir.display()
        )),
    }
}

pub fn write_default(engage_dir: &Path, name: &str) -> Result<std::path::PathBuf> {
    let mut p = MalleableProfile::default();
    if !name.is_empty() {
        p.name = name.into();
    }
    p.validate()?;
    let dir = engage_dir.join("profiles");
    fs::create_dir_all(&dir)?;
    let metadata = fs::symlink_metadata(&dir)
        .map_err(|e| anyhow!("ANUBIS_MALLEABLE_DIR_METADATA: {}: {e}", dir.display()))?;
    if metadata.file_type().is_symlink() {
        return Err(anyhow!("ANUBIS_MALLEABLE_DIR_SYMLINK: {}", dir.display()));
    }
    if !metadata.is_dir() {
        return Err(anyhow!(
            "ANUBIS_MALLEABLE_DIR_NOT_DIRECTORY: {}",
            dir.display()
        ));
    }
    let path = dir.join(format!("{}.json", sanitize(&p.name)));
    let existing = profile_paths(&dir)?;
    if existing.iter().any(|candidate| candidate != &path) {
        return Err(anyhow!(
            "ANUBIS_MALLEABLE_AMBIGUOUS: refusing to add {} while another JSON profile exists in {}; overwrite the active profile or remove it first",
            path.display(),
            dir.display()
        ));
    }
    fs::write(&path, serde_json::to_string_pretty(&p)?)?;
    Ok(path)
}

pub fn load(path: &Path) -> Result<MalleableProfile> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|e| anyhow!("ANUBIS_MALLEABLE_LOAD: {}: {e}", path.display()))?;
    if !metadata.file_type().is_file() {
        return Err(anyhow!(
            "ANUBIS_MALLEABLE_PROFILE_NOT_REGULAR: {}",
            path.display()
        ));
    }
    if metadata.len() > MAX_PROFILE_BYTES as u64 {
        return Err(anyhow!(
            "ANUBIS_MALLEABLE_PROFILE_TOO_LARGE: {} bytes (max {MAX_PROFILE_BYTES})",
            metadata.len()
        ));
    }
    let file = fs::File::open(path)
        .map_err(|e| anyhow!("ANUBIS_MALLEABLE_LOAD: {}: {e}", path.display()))?;
    let mut raw = Vec::with_capacity(metadata.len() as usize);
    file.take((MAX_PROFILE_BYTES + 1) as u64)
        .read_to_end(&mut raw)
        .map_err(|e| anyhow!("ANUBIS_MALLEABLE_LOAD: {}: {e}", path.display()))?;
    if raw.len() > MAX_PROFILE_BYTES {
        return Err(anyhow!(
            "ANUBIS_MALLEABLE_PROFILE_TOO_LARGE: grew beyond {MAX_PROFILE_BYTES} bytes while reading"
        ));
    }
    let p: MalleableProfile =
        serde_json::from_slice(&raw).map_err(|e| anyhow!("ANUBIS_MALLEABLE_PARSE: {e}"))?;
    p.validate()?;
    Ok(p)
}

pub fn validate_file(path: &Path) -> Result<serde_json::Value> {
    let p = load(path)?;
    Ok(json!({
        "ok": true,
        "profile": p,
        "path": path.display().to_string(),
        "attck": ["T1071", "T1090"],
    }))
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_profile_validates_ok() {
        let p = MalleableProfile::default();
        p.validate().expect("default profile should validate");
        assert_eq!(p.name, "aop-default-jquery");
        assert!(!p.beacon_uris.is_empty());
        assert!(p.user_agent.starts_with("Mozilla/5.0"));
        assert_eq!(p.transform, "none");
    }

    #[test]
    fn validate_rejects_empty_name() {
        let p = MalleableProfile {
            name: String::new(),
            ..MalleableProfile::default()
        };
        let err = p.validate().unwrap_err().to_string();
        assert!(err.contains("ANUBIS_MALLEABLE_NAME"), "{err}");
    }

    #[test]
    fn validate_rejects_no_beacon_uris() {
        let mut p = MalleableProfile::default();
        p.beacon_uris.clear();
        let err = p.validate().unwrap_err().to_string();
        assert!(err.contains("ANUBIS_MALLEABLE_NO_BEACON_URI"), "{err}");
    }

    #[test]
    fn validate_rejects_non_absolute_uri() {
        let p = MalleableProfile {
            beacon_uris: vec!["relative/path".into()],
            ..MalleableProfile::default()
        };
        let err = p.validate().unwrap_err().to_string();
        assert!(err.contains("ANUBIS_MALLEABLE_URI_MUST_ABS"), "{err}");
    }

    #[test]
    fn validate_rejects_traversal_uri() {
        let p = MalleableProfile {
            beacon_uris: vec!["/ok/../etc/passwd".into()],
            ..MalleableProfile::default()
        };
        let err = p.validate().unwrap_err().to_string();
        assert!(err.contains("ANUBIS_MALLEABLE_URI_HOSTILE"), "{err}");
    }

    #[test]
    fn validate_rejects_scheme_uri() {
        let p = MalleableProfile {
            beacon_uris: vec!["http://evil.com/beacon".into()],
            ..MalleableProfile::default()
        };
        let err = p.validate().unwrap_err().to_string();
        // Rejected by the absolute-path rule, which fires FIRST — the scheme check below it is
        // unreachable for this input. The security property (a scheme URI is refused) holds; only
        // the diagnostic differs from the one this test originally expected.
        assert!(err.contains("ANUBIS_MALLEABLE_URI_MUST_ABS"), "{err}");
    }

    #[test]
    fn validate_rejects_protocol_relative_uri() {
        // `//host/path` passed BOTH checks before the fix: it starts with `/` so it is "absolute",
        // and has no `://` so it is not "hostile" — while resolving to an arbitrary external host.
        for u in ["//evil.com/beacon", "/\\evil.com/beacon", "/a\\b"] {
            let p = MalleableProfile {
                beacon_uris: vec![u.into()],
                ..MalleableProfile::default()
            };
            let err = p.validate().unwrap_err().to_string();
            assert!(err.contains("ANUBIS_MALLEABLE_URI_HOSTILE"), "{u}: {err}");
        }
    }

    #[test]
    fn validate_rejects_long_user_agent() {
        let p = MalleableProfile {
            user_agent: "A".repeat(513),
            ..MalleableProfile::default()
        };
        let err = p.validate().unwrap_err().to_string();
        assert!(err.contains("ANUBIS_MALLEABLE_UA_LONG"), "{err}");
    }

    #[test]
    fn validate_rejects_unbounded_or_ambiguous_routes() {
        let too_many = MalleableProfile {
            beacon_uris: (0..33).map(|i| format!("/beacon-{i}")).collect(),
            ..MalleableProfile::default()
        };
        let err = too_many.validate().unwrap_err().to_string();
        assert!(err.contains("ANUBIS_MALLEABLE_URI_COUNT"), "{err}");

        let too_long = MalleableProfile {
            beacon_uris: vec![format!("/{}", "a".repeat(2048))],
            ..MalleableProfile::default()
        };
        let err = too_long.validate().unwrap_err().to_string();
        assert!(err.contains("ANUBIS_MALLEABLE_URI_LONG"), "{err}");

        let duplicate = MalleableProfile {
            beacon_uris: vec!["/same".into()],
            result_uris: vec!["/same".into()],
            ..MalleableProfile::default()
        };
        let err = duplicate.validate().unwrap_err().to_string();
        assert!(err.contains("ANUBIS_MALLEABLE_URI_DUPLICATE"), "{err}");
    }

    #[test]
    fn validate_rejects_unbounded_or_malformed_headers() {
        let too_many = MalleableProfile {
            headers: (0..65)
                .map(|i| [format!("X-Anubis-{i}"), "ok".into()])
                .collect(),
            ..MalleableProfile::default()
        };
        let err = too_many.validate().unwrap_err().to_string();
        assert!(err.contains("ANUBIS_MALLEABLE_HEADER_COUNT"), "{err}");

        for header in [
            ["Bad Header".into(), "ok".into()],
            ["X-Anubis".into(), "ok\r\nInjected: yes".into()],
            ["X-Anubis".into(), "v".repeat(4097)],
        ] {
            let p = MalleableProfile {
                headers: vec![header],
                ..MalleableProfile::default()
            };
            let err = p.validate().unwrap_err().to_string();
            assert!(err.contains("ANUBIS_MALLEABLE_HEADER"), "{err}");
        }
    }

    #[test]
    fn load_rejects_an_oversized_profile_before_parsing() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("profile.json");
        fs::write(&path, vec![b' '; 1_048_577]).expect("write oversized profile");
        let err = load(&path).unwrap_err().to_string();
        assert!(err.contains("ANUBIS_MALLEABLE_PROFILE_TOO_LARGE"), "{err}");
    }

    #[test]
    fn sanitize_replaces_special_chars() {
        assert_eq!(sanitize("hello world!@#"), "hello_world___");
        assert_eq!(sanitize("abc-def_123"), "abc-def_123");
    }

    #[test]
    fn transform_none_is_identity() {
        let p = MalleableProfile::default();
        assert_eq!(p.transform, "none");
        let data = b"hello world";
        assert_eq!(p.apply_transform(data).expect("none"), data);
    }

    #[test]
    fn transform_base64_encodes() {
        let p = MalleableProfile {
            transform: "base64".into(),
            ..Default::default()
        };
        let out = p.apply_transform(b"hello").expect("base64");
        assert_eq!(out, b"aGVsbG8=");
    }

    #[test]
    fn transform_prepend_junk_adds_prefix() {
        let p = MalleableProfile {
            transform: "prepend_junk".into(),
            ..Default::default()
        };
        let out = p.apply_transform(b"data").expect("prepend_junk");
        assert!(out.starts_with(b"<!-- cached -->\n"));
        assert!(out.ends_with(b"data"));
    }

    // The two tests above exercise apply_transform DIRECTLY. They stay green because the
    // listener code is real; they are not reachable through a validated profile, because
    // `base64`/`prepend_junk` have no beacon inverse. That gap is the subject of the tests
    // below, and the day the agent template learns an inverse these become end-to-end.

    /// Every name the schema recognises must have a real arm in `apply_transform`. Without
    /// this, adding a name to KNOWN_TRANSFORMS and forgetting the implementation lands an
    /// operator-visible option that silently does nothing — the exact fail-open this module
    /// just closed on the declaration surface.
    #[test]
    fn every_known_transform_has_a_real_implementation() {
        let probe = b"anubis-transform-probe";
        for name in KNOWN_TRANSFORMS {
            let p = MalleableProfile {
                transform: (*name).into(),
                ..Default::default()
            };
            let out = p.apply_transform(probe).expect("known transform");
            if *name == "none" {
                assert_eq!(out, probe, "`none` must be identity");
            } else {
                assert_ne!(
                    out, probe,
                    "transform `{name}` is listed in KNOWN_TRANSFORMS but apply_transform \
                     returned its input unchanged — it fell through the `_` arm, so an \
                     operator selecting it would shape nothing while believing otherwise",
                );
            }
        }
    }

    #[test]
    fn inverse_set_is_a_subset_of_known_transforms() {
        for name in TRANSFORMS_WITH_BEACON_INVERSE {
            assert!(
                KNOWN_TRANSFORMS.contains(name),
                "`{name}` is claimed reversible but is not a recognised transform",
            );
        }
    }

    #[test]
    fn validate_rejects_an_unknown_transform() {
        // trailing space — the typo that used to pass
        let p = MalleableProfile {
            transform: "base64 ".into(),
            ..Default::default()
        };
        let err = p.validate().unwrap_err().to_string();
        assert!(
            err.contains("ANUBIS_MALLEABLE_TRANSFORM_UNKNOWN"),
            "a typo'd transform must be refused, not silently treated as identity: {err}",
        );
    }

    #[test]
    fn apply_transform_rejects_unknown_even_without_prior_validation() {
        let p = MalleableProfile {
            transform: "typo".into(),
            ..Default::default()
        };
        let err = p
            .apply_transform(b"data")
            .expect_err("the final consumer must not treat an unknown transform as identity")
            .to_string();
        assert!(err.contains("ANUBIS_MALLEABLE_TRANSFORM_UNKNOWN"), "{err}");
    }

    #[test]
    fn validate_rejects_a_transform_the_beacon_cannot_reverse() {
        for name in ["base64", "prepend_junk"] {
            let p = MalleableProfile {
                transform: name.into(),
                ..Default::default()
            };
            let err = p.validate().unwrap_err().to_string();
            assert!(
                err.contains("ANUBIS_MALLEABLE_TRANSFORM_NO_INVERSE"),
                "`{name}` transforms every beacon response with no beacon-side inverse; \
                 validate must refuse it rather than ship a silent C2 break: {err}",
            );
        }
    }

    /// An INVALID profile on disk must be an error, not an absent profile. The listener
    /// treats `Ok(None)` as "operator wrote none" and comes up with built-in behaviour; if a
    /// rejected profile collapsed to `None`, the operator's traffic shaping would be silently
    /// off with the listener reporting nothing wrong.
    #[test]
    fn load_from_engage_errors_on_an_invalid_profile_rather_than_reporting_none() {
        let dir = std::env::temp_dir().join(format!(
            "anubis-malleable-{}-{}",
            std::process::id(),
            "invalid"
        ));
        let profiles = dir.join("profiles");
        fs::create_dir_all(&profiles).expect("temp profiles dir");

        // no profile at all -> Ok(None), the legitimate "operator wrote none" case
        match load_from_engage(&dir) {
            Ok(None) => {}
            other => panic!(
                "empty profiles dir must be Ok(None), got {:?}",
                other.is_ok()
            ),
        }

        // valid JSON, refused by validate: no beacon inverse
        let bad = MalleableProfile {
            transform: "base64".into(),
            ..Default::default()
        };
        fs::write(
            profiles.join("bad.json"),
            serde_json::to_string(&bad).expect("serialize"),
        )
        .expect("write bad profile");

        let err = load_from_engage(&dir)
            .expect_err("an invalid profile must surface as Err, never as Ok(None)")
            .to_string();
        assert!(
            err.contains("ANUBIS_MALLEABLE_TRANSFORM_NO_INVERSE"),
            "the error must name why the profile was refused: {err}",
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_rejects_unknown_profile_keys_instead_of_defaulting_a_typo() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("profile.json");
        let mut value = serde_json::to_value(MalleableProfile::default()).expect("serialize");
        let object = value.as_object_mut().expect("profile object");
        object.remove("transform");
        object.insert(
            "tranform".into(),
            serde_json::Value::String("base64".into()),
        );
        fs::write(&path, serde_json::to_vec(&value).expect("json")).expect("write profile");

        let err = load(&path)
            .expect_err("an unknown key must not silently select the default transform")
            .to_string();
        assert!(err.contains("ANUBIS_MALLEABLE_PARSE"), "{err}");
        assert!(err.contains("unknown field"), "{err}");
    }

    #[test]
    fn validate_rejects_unsupported_schema_version() {
        let p = MalleableProfile {
            schema_version: "2.0".into(),
            ..Default::default()
        };
        let err = p
            .validate()
            .expect_err("unsupported schemas must fail closed")
            .to_string();
        assert!(err.contains("ANUBIS_MALLEABLE_SCHEMA_VERSION"), "{err}");
    }

    #[test]
    fn load_from_engage_rejects_profiles_path_that_is_not_a_directory() {
        let dir = tempfile::tempdir().expect("temp dir");
        fs::write(dir.path().join("profiles"), b"not a directory").expect("write marker");
        let err = load_from_engage(dir.path())
            .expect_err("an invalid profiles path must not mean no profile")
            .to_string();
        assert!(err.contains("ANUBIS_MALLEABLE_DIR_NOT_DIRECTORY"), "{err}");
    }

    #[test]
    fn load_from_engage_rejects_multiple_json_profiles_as_ambiguous() {
        let dir = tempfile::tempdir().expect("temp dir");
        let profiles = dir.path().join("profiles");
        fs::create_dir(&profiles).expect("profiles dir");
        fs::write(
            profiles.join("a-valid.json"),
            serde_json::to_vec(&MalleableProfile::default()).expect("serialize"),
        )
        .expect("write valid profile");
        fs::write(profiles.join("z-malformed.json"), b"{").expect("write malformed profile");

        let err = load_from_engage(dir.path())
            .expect_err("multiple profiles must not select the lexicographically first")
            .to_string();
        assert!(err.contains("ANUBIS_MALLEABLE_AMBIGUOUS"), "{err}");
    }

    #[test]
    fn load_from_engage_rejects_a_json_symlink() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().expect("temp dir");
        let profiles = dir.path().join("profiles");
        fs::create_dir(&profiles).expect("profiles dir");
        let target = dir.path().join("outside.json");
        fs::write(
            &target,
            serde_json::to_vec(&MalleableProfile::default()).expect("serialize"),
        )
        .expect("write target");
        symlink(&target, profiles.join("profile.json")).expect("create symlink");

        let err = load_from_engage(dir.path())
            .expect_err("a profile symlink must not escape the engagement profile directory")
            .to_string();
        assert!(
            err.contains("ANUBIS_MALLEABLE_PROFILE_NOT_REGULAR"),
            "{err}"
        );
    }

    #[test]
    fn load_from_engage_rejects_a_symlinked_profiles_directory() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().expect("temp dir");
        let outside = tempfile::tempdir().expect("outside dir");
        fs::write(
            outside.path().join("profile.json"),
            serde_json::to_vec(&MalleableProfile::default()).expect("serialize"),
        )
        .expect("write profile");
        symlink(outside.path(), dir.path().join("profiles")).expect("symlink profiles");
        let err = load_from_engage(dir.path())
            .expect_err("a profiles directory symlink must be rejected")
            .to_string();
        assert!(err.contains("ANUBIS_MALLEABLE_DIR_SYMLINK"), "{err}");
    }

    #[test]
    fn load_from_engage_rejects_a_broken_profiles_symlink() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().expect("temp dir");
        symlink(
            dir.path().join("missing-target"),
            dir.path().join("profiles"),
        )
        .expect("symlink profiles");
        let err = load_from_engage(dir.path())
            .expect_err("a broken profiles symlink is not an absent profile")
            .to_string();
        assert!(err.contains("ANUBIS_MALLEABLE_DIR_SYMLINK"), "{err}");
    }

    #[test]
    fn write_default_rejects_a_second_differently_named_profile() {
        let dir = tempfile::tempdir().expect("temp dir");
        write_default(dir.path(), "first").expect("first profile");
        let err = write_default(dir.path(), "second")
            .expect_err("the writer must not create a state the loader rejects")
            .to_string();
        assert!(err.contains("ANUBIS_MALLEABLE_AMBIGUOUS"), "{err}");
    }

    #[test]
    fn write_default_allows_intentional_same_name_overwrite() {
        let dir = tempfile::tempdir().expect("temp dir");
        let first = write_default(dir.path(), "same").expect("first profile");
        let second = write_default(dir.path(), "same").expect("overwrite same profile");
        assert_eq!(first, second);
        assert!(load_from_engage(dir.path()).expect("load").is_some());
    }

    /// The default profile is what `write_default` puts on disk and what the listener loads
    /// when an operator has not written one. It must survive the new checks.
    #[test]
    fn default_profile_still_validates() {
        MalleableProfile::default().validate().expect(
            "default profile must validate — write_default and the listener both rely on it",
        );
    }

    #[test]
    fn format_server_headers_emits_crlf() {
        let p = MalleableProfile::default();
        let hdr = p.format_server_headers();
        assert!(hdr.contains("Server: nginx\r\n"));
        assert!(hdr.contains("X-Content-Type-Options: nosniff\r\n"));
    }
}
