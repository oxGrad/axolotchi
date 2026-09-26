//! Ethernet II frame encode/decode. Pure and hardware-free — the raw socket
//! transport (`raw_socket.rs`) just moves the bytes these functions produce
//! and consume.

pub const MAC_LEN: usize = 6;
pub type MacAddr = [u8; MAC_LEN];

pub const BROADCAST_MAC: MacAddr = [0xff; MAC_LEN];
pub const ETHERTYPE_ARP: u16 = 0x0806;
pub const ETHERTYPE_IPV4: u16 = 0x0800;

const HEADER_LEN: usize = 14;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EthernetFrame {
    pub dst: MacAddr,
    pub src: MacAddr,
    pub ethertype: u16,
    pub payload: Vec<u8>,
}

impl EthernetFrame {
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(HEADER_LEN + self.payload.len());
        buf.extend_from_slice(&self.dst);
        buf.extend_from_slice(&self.src);
        buf.extend_from_slice(&self.ethertype.to_be_bytes());
        buf.extend_from_slice(&self.payload);
        buf
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < HEADER_LEN {
            return None;
        }
        let mut dst = [0u8; MAC_LEN];
        dst.copy_from_slice(&bytes[0..6]);
        let mut src = [0u8; MAC_LEN];
        src.copy_from_slice(&bytes[6..12]);
        let ethertype = u16::from_be_bytes([bytes[12], bytes[13]]);
        Some(Self {
            dst,
            src,
            ethertype,
            payload: bytes[HEADER_LEN..].to_vec(),
        })
    }
}

pub fn format_mac(mac: &MacAddr) -> String {
    mac.iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

/// Parses a MAC address string, accepting `:` or `-` separated hex octets
/// (case-insensitive). Returns `None` for anything else, rather than
/// panicking on malformed input from the network or a config file.
pub fn parse_mac(s: &str) -> Option<MacAddr> {
    let mut mac = [0u8; MAC_LEN];
    let mut parts = s.split([':', '-']);
    for slot in mac.iter_mut() {
        let part = parts.next()?;
        *slot = u8::from_str_radix(part, 16).ok()?;
    }
    if parts.next().is_some() {
        return None;
    }
    Some(mac)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_frame() {
        let frame = EthernetFrame {
            dst: BROADCAST_MAC,
            src: [0x02, 0x11, 0x22, 0x33, 0x44, 0x55],
            ethertype: ETHERTYPE_ARP,
            payload: vec![1, 2, 3, 4],
        };
        let bytes = frame.encode();
        assert_eq!(EthernetFrame::decode(&bytes), Some(frame));
    }

    #[test]
    fn decode_rejects_short_input() {
        assert_eq!(EthernetFrame::decode(&[0u8; 13]), None);
    }

    #[test]
    fn decode_accepts_an_empty_payload() {
        let bytes = vec![0u8; HEADER_LEN];
        let frame = EthernetFrame::decode(&bytes).unwrap();
        assert!(frame.payload.is_empty());
    }

    #[test]
    fn format_and_parse_mac_round_trip() {
        let mac: MacAddr = [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff];
        let formatted = format_mac(&mac);
        assert_eq!(formatted, "aa:bb:cc:dd:ee:ff");
        assert_eq!(parse_mac(&formatted), Some(mac));
    }

    #[test]
    fn parse_mac_accepts_dash_separators_and_uppercase() {
        assert_eq!(
            parse_mac("AA-BB-CC-DD-EE-FF"),
            Some([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff])
        );
    }

    #[test]
    fn parse_mac_rejects_malformed_input() {
        assert_eq!(parse_mac("not-a-mac"), None);
        assert_eq!(parse_mac("aa:bb:cc:dd:ee"), None);
        assert_eq!(parse_mac("aa:bb:cc:dd:ee:ff:00"), None);
    }
}
