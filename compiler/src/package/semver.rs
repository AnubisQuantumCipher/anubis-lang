//! Minimal SemVer 2.0 parse/compare + caret/tilde/exact ranges for package resolution.

use std::cmp::Ordering;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub pre: String,
}

impl Version {
    pub fn parse(s: &str) -> Result<Version, String> {
        let s = s.trim().trim_start_matches('v');
        let (core, pre) = match s.split_once('-') {
            Some((c, p)) => (c, p.to_string()),
            None => (s, String::new()),
        };
        let parts: Vec<&str> = core.split('.').collect();
        if parts.is_empty() || parts.len() > 3 {
            return Err(format!("ANUBIS_DEP_UNRESOLVED: invalid version `{s}`"));
        }
        let major = parts[0]
            .parse()
            .map_err(|_| format!("ANUBIS_DEP_UNRESOLVED: invalid version `{s}`"))?;
        let minor = if parts.len() > 1 {
            parts[1]
                .parse()
                .map_err(|_| format!("ANUBIS_DEP_UNRESOLVED: invalid version `{s}`"))?
        } else {
            0
        };
        let patch = if parts.len() > 2 {
            parts[2]
                .parse()
                .map_err(|_| format!("ANUBIS_DEP_UNRESOLVED: invalid version `{s}`"))?
        } else {
            0
        };
        Ok(Version {
            major,
            minor,
            patch,
            pre,
        })
    }

    pub fn to_string_canonical(&self) -> String {
        if self.pre.is_empty() {
            format!("{}.{}.{}", self.major, self.minor, self.patch)
        } else {
            format!("{}.{}.{}-{}", self.major, self.minor, self.patch, self.pre)
        }
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.major
            .cmp(&other.major)
            .then(self.minor.cmp(&other.minor))
            .then(self.patch.cmp(&other.patch))
            .then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater, // release > prerelease
                (false, true) => Ordering::Less,
                (false, false) => self.pre.cmp(&other.pre),
            })
    }
}

/// A version requirement: exact, caret `^`, tilde `~`, or bare `1.2` (= `^1.2.0` if 2-part).
#[derive(Debug, Clone)]
pub enum VersionReq {
    Exact(Version),
    Caret(Version),
    Tilde(Version),
    Star,
}

impl VersionReq {
    pub fn parse(s: &str) -> Result<VersionReq, String> {
        let s = s.trim();
        if s == "*" {
            return Ok(VersionReq::Star);
        }
        if let Some(rest) = s.strip_prefix('^') {
            return Ok(VersionReq::Caret(Version::parse(rest)?));
        }
        if let Some(rest) = s.strip_prefix('~') {
            return Ok(VersionReq::Tilde(Version::parse(rest)?));
        }
        if let Some(rest) = s.strip_prefix('=') {
            return Ok(VersionReq::Exact(Version::parse(rest)?));
        }
        // Bare version: treat as caret (Cargo-like for `"1.2"`).
        Ok(VersionReq::Caret(Version::parse(s)?))
    }

    pub fn matches(&self, v: &Version) -> bool {
        match self {
            VersionReq::Star => true,
            VersionReq::Exact(e) => v == e,
            VersionReq::Caret(base) => {
                if !v.pre.is_empty() {
                    return false;
                }
                if base.major > 0 {
                    v.major == base.major && v >= base
                } else if base.minor > 0 {
                    v.major == 0 && v.minor == base.minor && v >= base
                } else {
                    v.major == 0 && v.minor == 0 && v.patch == base.patch
                }
            }
            VersionReq::Tilde(base) => {
                if !v.pre.is_empty() {
                    return false;
                }
                v.major == base.major && v.minor == base.minor && v >= base
            }
        }
    }
}

/// Pick the highest version from `candidates` matching `req`.
pub fn select_max_matching<'a>(
    req: &VersionReq,
    candidates: impl IntoIterator<Item = &'a str>,
) -> Result<Version, String> {
    let mut best: Option<Version> = None;
    for c in candidates {
        if let Ok(v) = Version::parse(c) {
            if req.matches(&v) {
                best = Some(match best {
                    None => v,
                    Some(b) if v > b => v,
                    Some(b) => b,
                });
            }
        }
    }
    best.ok_or_else(|| "ANUBIS_REGISTRY_MISS: no version matches requirement".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caret_matches() {
        let r = VersionReq::parse("^1.2.0").unwrap();
        assert!(r.matches(&Version::parse("1.2.0").unwrap()));
        assert!(r.matches(&Version::parse("1.9.9").unwrap()));
        assert!(!r.matches(&Version::parse("2.0.0").unwrap()));
        assert!(!r.matches(&Version::parse("1.1.9").unwrap()));
    }

    #[test]
    fn select_max() {
        let r = VersionReq::parse("^1.0").unwrap();
        let v = select_max_matching(&r, ["1.0.0", "1.2.3", "0.9.0", "2.0.0"]).unwrap();
        assert_eq!(v.to_string_canonical(), "1.2.3");
    }
}
