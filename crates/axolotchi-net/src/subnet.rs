//! Lists the host addresses an ARP sweep should probe. Pure — no socket
//! involved.

use std::net::Ipv4Addr;

/// A Pi Zero doing an active sweep shouldn't be asked to probe more hosts
/// than a typical home LAN has; this also keeps a misconfigured wide CIDR
/// from producing a multi-million-entry `Vec`.
pub const MAX_SWEEP_HOSTS: usize = 4096;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SubnetError {
    #[error("invalid CIDR prefix length: {0} (must be 0-32)")]
    InvalidPrefix(u8),
    #[error("subnet too large to sweep: {hosts} hosts exceeds the {max} limit")]
    TooLarge { hosts: u64, max: usize },
}

/// Lists the usable host addresses in an IPv4 CIDR block, excluding the
/// network and broadcast addresses for prefixes shorter than /31 (which,
/// per RFC 3021, have no network/broadcast address to exclude).
pub fn hosts(network: Ipv4Addr, prefix_len: u8) -> Result<Vec<Ipv4Addr>, SubnetError> {
    if prefix_len > 32 {
        return Err(SubnetError::InvalidPrefix(prefix_len));
    }

    let host_bits = 32 - prefix_len as u32;
    let total: u64 = 1u64 << host_bits;
    if total > MAX_SWEEP_HOSTS as u64 {
        return Err(SubnetError::TooLarge {
            hosts: total,
            max: MAX_SWEEP_HOSTS,
        });
    }

    // Safe: the size check above guarantees host_bits is small (<= 12),
    // well clear of the shift-amount-overflow case.
    let mask = !0u32 << host_bits;
    let base = u32::from(network) & mask;

    let (first, last) = if prefix_len >= 31 {
        (0, total as u32 - 1)
    } else {
        (1, total as u32 - 2)
    };

    Ok((first..=last)
        .map(|offset| Ipv4Addr::from(base + offset))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slash_24_excludes_network_and_broadcast() {
        let addrs = hosts(Ipv4Addr::new(192, 168, 1, 0), 24).unwrap();
        assert_eq!(addrs.len(), 254);
        assert_eq!(addrs.first(), Some(&Ipv4Addr::new(192, 168, 1, 1)));
        assert_eq!(addrs.last(), Some(&Ipv4Addr::new(192, 168, 1, 254)));
    }

    #[test]
    fn slash_30_gives_two_usable_hosts() {
        let addrs = hosts(Ipv4Addr::new(10, 0, 0, 0), 30).unwrap();
        assert_eq!(
            addrs,
            vec![Ipv4Addr::new(10, 0, 0, 1), Ipv4Addr::new(10, 0, 0, 2)]
        );
    }

    #[test]
    fn slash_31_has_no_network_or_broadcast_to_exclude() {
        let addrs = hosts(Ipv4Addr::new(10, 0, 0, 0), 31).unwrap();
        assert_eq!(
            addrs,
            vec![Ipv4Addr::new(10, 0, 0, 0), Ipv4Addr::new(10, 0, 0, 1)]
        );
    }

    #[test]
    fn slash_32_is_a_single_host() {
        let addrs = hosts(Ipv4Addr::new(10, 0, 0, 5), 32).unwrap();
        assert_eq!(addrs, vec![Ipv4Addr::new(10, 0, 0, 5)]);
    }

    #[test]
    fn prefix_over_32_is_rejected() {
        assert_eq!(
            hosts(Ipv4Addr::new(10, 0, 0, 0), 33),
            Err(SubnetError::InvalidPrefix(33))
        );
    }

    #[test]
    fn oversized_subnet_is_rejected() {
        let result = hosts(Ipv4Addr::new(10, 0, 0, 0), 8);
        assert!(matches!(result, Err(SubnetError::TooLarge { .. })));
    }
}
