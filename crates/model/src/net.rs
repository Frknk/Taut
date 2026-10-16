//! Which network addresses a monitor may target.
//!
//! Monitors must not reach loopback, private, link-local or other non-public ranges unless
//! the deployment allows it (`checks.allow_private_targets`); otherwise a tenant could use
//! the checkers to scan the cluster's internal network. The API applies these rules to the
//! target as written and the checker applies them again to every resolved address, which
//! also covers hostnames that resolve to internal IPs.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Whether `ip` belongs to a range that monitors may not target by default.
pub fn is_internal_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_internal_ipv4(ip),
        IpAddr::V6(ip) => ip
            .to_ipv4_mapped()
            .map_or_else(|| is_internal_ipv6(ip), is_internal_ipv4),
    }
}

fn is_internal_ipv4(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
        || a == 0
        // 100.64.0.0/10, carrier-grade NAT; also used by some clusters for pod networks.
        || (a == 100 && (b & 0b1100_0000) == 64)
}

fn is_internal_ipv6(ip: Ipv6Addr) -> bool {
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || ip.is_unique_local()
        || ip.is_unicast_link_local()
}

/// Whether `host` (a hostname or an IP literal, IPv6 with or without brackets) names an
/// internal address without needing DNS.
pub fn is_internal_host(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host == "localhost" || host.ends_with(".localhost") {
        return true;
    }
    let bare = host
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or(&host);
    bare.parse::<IpAddr>().is_ok_and(is_internal_ip)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_ranges_are_internal() {
        for ip in [
            "10.1.2.3",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.1",
            "169.254.169.254",
            "127.0.0.1",
            "0.0.0.0",
            "100.64.0.1",
            "::1",
            "fd00::1",
            "fe80::1",
            "::ffff:10.0.0.1",
        ] {
            assert!(
                is_internal_ip(ip.parse().unwrap()),
                "{ip} should be internal"
            );
        }
    }

    #[test]
    fn public_addresses_are_not_internal() {
        for ip in [
            "1.1.1.1",
            "8.8.8.8",
            "172.32.0.1",
            "100.128.0.1",
            "2606:4700::1111",
        ] {
            assert!(
                !is_internal_ip(ip.parse().unwrap()),
                "{ip} should be public"
            );
        }
    }

    #[test]
    fn hosts_are_classified_without_dns() {
        assert!(is_internal_host("localhost"));
        assert!(is_internal_host("api.localhost"));
        assert!(is_internal_host("LOCALHOST."));
        assert!(is_internal_host("127.0.0.1"));
        assert!(is_internal_host("[::1]"));
        assert!(!is_internal_host("example.com"));
        assert!(!is_internal_host("1.1.1.1"));
    }
}
