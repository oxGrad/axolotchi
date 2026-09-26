//! Just enough IPv4 header parsing to pull a source address out of a
//! passively sniffed frame. Pure — no socket involved.

use std::net::Ipv4Addr;

const MIN_HEADER_LEN: usize = 20;

/// Returns the source address of an IPv4 packet, or `None` if `bytes` is
/// too short or not IPv4.
pub fn parse_src(bytes: &[u8]) -> Option<Ipv4Addr> {
    if bytes.len() < MIN_HEADER_LEN {
        return None;
    }
    let version = bytes[0] >> 4;
    if version != 4 {
        return None;
    }
    Some(Ipv4Addr::new(bytes[12], bytes[13], bytes[14], bytes[15]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header_with_src(src: [u8; 4]) -> Vec<u8> {
        let mut bytes = vec![0u8; MIN_HEADER_LEN];
        bytes[0] = 0x45; // version 4, 20-byte header
        bytes[12..16].copy_from_slice(&src);
        bytes
    }

    #[test]
    fn extracts_source_address() {
        let bytes = header_with_src([10, 0, 0, 5]);
        assert_eq!(parse_src(&bytes), Some(Ipv4Addr::new(10, 0, 0, 5)));
    }

    #[test]
    fn rejects_short_input() {
        assert_eq!(parse_src(&[0u8; MIN_HEADER_LEN - 1]), None);
    }

    #[test]
    fn rejects_non_ipv4_version() {
        let mut bytes = header_with_src([10, 0, 0, 5]);
        bytes[0] = 0x65; // version 6
        assert_eq!(parse_src(&bytes), None);
    }
}
