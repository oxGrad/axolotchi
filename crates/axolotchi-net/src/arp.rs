//! ARP packet (RFC 826, Ethernet/IPv4 only) encode/decode. Pure — no socket
//! involved.

use crate::ethernet::MacAddr;
use std::net::Ipv4Addr;

pub const ARP_LEN: usize = 28;

const HTYPE_ETHERNET: u16 = 1;
const PTYPE_IPV4: u16 = 0x0800;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArpOperation {
    Request,
    Reply,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArpPacket {
    pub operation: ArpOperation,
    pub sender_mac: MacAddr,
    pub sender_ip: Ipv4Addr,
    pub target_mac: MacAddr,
    pub target_ip: Ipv4Addr,
}

impl ArpPacket {
    pub fn encode(&self) -> [u8; ARP_LEN] {
        let mut buf = [0u8; ARP_LEN];
        buf[0..2].copy_from_slice(&HTYPE_ETHERNET.to_be_bytes());
        buf[2..4].copy_from_slice(&PTYPE_IPV4.to_be_bytes());
        buf[4] = 6; // hardware address length
        buf[5] = 4; // protocol address length
        let op: u16 = match self.operation {
            ArpOperation::Request => 1,
            ArpOperation::Reply => 2,
        };
        buf[6..8].copy_from_slice(&op.to_be_bytes());
        buf[8..14].copy_from_slice(&self.sender_mac);
        buf[14..18].copy_from_slice(&self.sender_ip.octets());
        buf[18..24].copy_from_slice(&self.target_mac);
        buf[24..28].copy_from_slice(&self.target_ip.octets());
        buf
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < ARP_LEN {
            return None;
        }
        let htype = u16::from_be_bytes([bytes[0], bytes[1]]);
        let ptype = u16::from_be_bytes([bytes[2], bytes[3]]);
        let (hlen, plen) = (bytes[4], bytes[5]);
        if htype != HTYPE_ETHERNET || ptype != PTYPE_IPV4 || hlen != 6 || plen != 4 {
            return None;
        }
        let operation = match u16::from_be_bytes([bytes[6], bytes[7]]) {
            1 => ArpOperation::Request,
            2 => ArpOperation::Reply,
            _ => return None,
        };
        let mut sender_mac = [0u8; 6];
        sender_mac.copy_from_slice(&bytes[8..14]);
        let sender_ip = Ipv4Addr::new(bytes[14], bytes[15], bytes[16], bytes[17]);
        let mut target_mac = [0u8; 6];
        target_mac.copy_from_slice(&bytes[18..24]);
        let target_ip = Ipv4Addr::new(bytes[24], bytes[25], bytes[26], bytes[27]);
        Some(Self {
            operation,
            sender_mac,
            sender_ip,
            target_mac,
            target_ip,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(operation: ArpOperation) -> ArpPacket {
        ArpPacket {
            operation,
            sender_mac: [0x02, 0x11, 0x22, 0x33, 0x44, 0x55],
            sender_ip: Ipv4Addr::new(192, 168, 1, 1),
            target_mac: [0u8; 6],
            target_ip: Ipv4Addr::new(192, 168, 1, 42),
        }
    }

    #[test]
    fn round_trips_a_request() {
        let packet = packet(ArpOperation::Request);
        assert_eq!(ArpPacket::decode(&packet.encode()), Some(packet));
    }

    #[test]
    fn round_trips_a_reply() {
        let packet = packet(ArpOperation::Reply);
        assert_eq!(ArpPacket::decode(&packet.encode()), Some(packet));
    }

    #[test]
    fn decode_rejects_short_input() {
        assert_eq!(ArpPacket::decode(&[0u8; ARP_LEN - 1]), None);
    }

    #[test]
    fn decode_rejects_non_ethernet_ipv4() {
        let mut bytes = packet(ArpOperation::Request).encode();
        bytes[1] = 0; // corrupt hardware type (was big-endian 0x0001)
        assert_eq!(ArpPacket::decode(&bytes), None);
    }

    #[test]
    fn decode_rejects_unknown_operation() {
        let mut bytes = packet(ArpOperation::Request).encode();
        bytes[7] = 9; // not request(1) or reply(2)
        assert_eq!(ArpPacket::decode(&bytes), None);
    }
}
