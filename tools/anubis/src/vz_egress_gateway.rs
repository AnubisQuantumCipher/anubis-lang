//! DNS-pinned per-hostname egress policy for the native VZ file-handle NIC substrate.
//!
//! This is the userspace GATEWAY that turns the host end of a
//! `VZFileHandleNetworkDeviceAttachment` socketpair into a fail-closed allow-list.
//! Empty allow-list = deny-all (enforced).
//!
//! Frame filter: IPv4 TCP/UDP destination IP must resolve (via getaddrinfo at policy
//! build time) to an allow-listed hostname. Non-IPv4 L3 is dropped. Raw Ethernet frames
//! are inspected for EtherType IPv4 (0x0800); others dropped.
//!
//! Native-boot wires this process against the host fd; unit tests exercise the pure
//! policy without a live VM.

use anyhow::{anyhow, Result};
use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, ToSocketAddrs};

/// Compiled egress policy: destination IPv4 addresses derived from allow-listed hostnames.
#[derive(Debug, Clone)]
pub struct EgressPolicy {
    /// Original hostnames (for diagnostics).
    pub hosts: Vec<String>,
    /// Allowed IPv4 destinations (DNS-pinned at policy build).
    pub allowed_ipv4: HashSet<Ipv4Addr>,
}

impl EgressPolicy {
    /// Build policy. Empty host list → deny-all (allowed set empty).
    /// Fail-closed: a hostname that does not resolve is an error (operator mistype),
    /// not a silent open.
    pub fn from_allow_hosts(hosts: &[String]) -> Result<Self> {
        let mut allowed_ipv4 = HashSet::new();
        for h in hosts {
            let h = h.trim();
            if h.is_empty() {
                continue;
            }
            // Pin: resolve now; runtime filter uses IPs only (no re-resolve race).
            let addrs: Vec<_> = (h, 0u16)
                .to_socket_addrs()
                .map_err(|e| anyhow!("ANUBIS_EGRESS_DNS: resolve `{h}` failed: {e}"))?
                .collect();
            if addrs.is_empty() {
                return Err(anyhow!("ANUBIS_EGRESS_DNS: resolve `{h}` returned no addresses"));
            }
            let mut got_v4 = false;
            for a in addrs {
                if let IpAddr::V4(v4) = a.ip() {
                    allowed_ipv4.insert(v4);
                    got_v4 = true;
                }
            }
            if !got_v4 {
                return Err(anyhow!(
                    "ANUBIS_EGRESS_DNS: `{h}` resolved without IPv4 (IPv6-only not admitted)"
                ));
            }
        }
        Ok(Self {
            hosts: hosts.to_vec(),
            allowed_ipv4,
        })
    }

    /// Empty allow-list denies every IPv4 destination.
    pub fn permits_ipv4(&self, dst: Ipv4Addr) -> bool {
        self.allowed_ipv4.contains(&dst)
    }

    /// Inspect a raw Ethernet frame (no FCS). Return true if it may be forwarded.
    pub fn permits_ethernet_frame(&self, frame: &[u8]) -> bool {
        // Need dst MAC(6) + src MAC(6) + ethertype(2) + IPv4 header (>=20)
        if frame.len() < 14 + 20 {
            return false;
        }
        let ethertype = u16::from_be_bytes([frame[12], frame[13]]);
        if ethertype != 0x0800 {
            // Non-IPv4 (ARP, IPv6, …) — fail-closed drop.
            return false;
        }
        let ip = &frame[14..];
        let version = ip[0] >> 4;
        if version != 4 {
            return false;
        }
        let dst = Ipv4Addr::new(ip[16], ip[17], ip[18], ip[19]);
        self.permits_ipv4(dst)
    }
}

/// Build a minimal Ethernet+IPv4 frame for tests (zero MACs, IPv4 ethertype, dst IP).
pub fn test_ipv4_frame(dst: Ipv4Addr) -> Vec<u8> {
    let mut f = vec![0u8; 14 + 20];
    f[12] = 0x08;
    f[13] = 0x00;
    f[14] = 0x45; // v4, IHL=5
    let o = dst.octets();
    f[30] = o[0];
    f[31] = o[1];
    f[32] = o[2];
    f[33] = o[3];
    // IPv4 dest is at offset 16 within IP header = frame[14+16]=frame[30]
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_allow_list_denies_all() {
        let p = EgressPolicy::from_allow_hosts(&[]).unwrap();
        assert!(p.allowed_ipv4.is_empty());
        assert!(!p.permits_ipv4(Ipv4Addr::new(1, 1, 1, 1)));
        let frame = test_ipv4_frame(Ipv4Addr::new(8, 8, 8, 8));
        assert!(!p.permits_ethernet_frame(&frame));
    }

    #[test]
    fn loopback_allow_list_permits_only_pinned_ip() {
        let p = EgressPolicy::from_allow_hosts(&["localhost".into()]).unwrap();
        assert!(
            p.permits_ipv4(Ipv4Addr::LOCALHOST) || p.allowed_ipv4.iter().any(|ip| ip.is_loopback()),
            "localhost should pin a loopback v4"
        );
        // Arbitrary public IP not in pin set.
        assert!(!p.permits_ipv4(Ipv4Addr::new(203, 0, 113, 1)));
    }

    #[test]
    fn non_ipv4_ethertype_dropped() {
        let p = EgressPolicy::from_allow_hosts(&[]).unwrap();
        let mut frame = test_ipv4_frame(Ipv4Addr::LOCALHOST);
        frame[12] = 0x86; // IPv6 ethertype high
        frame[13] = 0xdd;
        assert!(!p.permits_ethernet_frame(&frame));
    }

    #[test]
    fn short_frame_dropped() {
        let p = EgressPolicy::from_allow_hosts(&[]).unwrap();
        assert!(!p.permits_ethernet_frame(&[0u8; 10]));
    }
}
