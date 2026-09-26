//! Passive sniffing: any Ethernet frame from a device is itself evidence
//! it's alive, letting a device get rescued between active sweeps. Opens up
//! an IP too when the frame happens to carry one (ARP or plain IPv4) — full
//! DHCP/mDNS payload parsing is a further enhancement, not required for the
//! presence state machine's passive-rescue behavior. Generic over
//! `Transport`, so it's fully testable with a fake.

use crate::arp::ArpPacket;
use crate::ethernet::{
    format_mac, EthernetFrame, MacAddr, BROADCAST_MAC, ETHERTYPE_ARP, ETHERTYPE_IPV4,
};
use crate::ipv4;
use crate::transport::Transport;
use axolotchi_core::{Event, SightingSource};
use std::time::Duration;
use tokio::sync::mpsc;

/// Pulls a passive sighting out of a raw Ethernet frame, ignoring frames
/// Axo itself sent (or anything claiming to be from the broadcast address,
/// which is never a legitimate frame source).
fn passive_sighting_from_frame(bytes: &[u8], our_mac: MacAddr, at: i64) -> Option<Event> {
    let frame = EthernetFrame::decode(bytes)?;
    if frame.src == our_mac || frame.src == BROADCAST_MAC {
        return None;
    }

    let ip = match frame.ethertype {
        ETHERTYPE_ARP => ArpPacket::decode(&frame.payload).map(|arp| arp.sender_ip.to_string()),
        ETHERTYPE_IPV4 => ipv4::parse_src(&frame.payload).map(|ip| ip.to_string()),
        _ => None,
    };

    Some(Event::Sighting {
        device_id: format_mac(&frame.src),
        ip,
        source: SightingSource::Passive,
        at,
    })
}

fn unix_now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Reads frames forever, turning each one into a passive sighting.
pub async fn run_passive_sniffer<T: Transport>(mut transport: T, tx: mpsc::Sender<Event>) {
    let our_mac = transport.local_mac();
    loop {
        match transport.recv_frame() {
            Ok(Some(bytes)) => {
                if let Some(event) = passive_sighting_from_frame(&bytes, our_mac, unix_now()) {
                    if tx.send(event).await.is_err() {
                        return;
                    }
                }
            }
            Ok(None) => tokio::time::sleep(Duration::from_millis(20)).await,
            Err(error) => {
                tracing::warn!(%error, "passive sniffer read failed");
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arp::ArpOperation;
    use crate::transport::FakeTransport;
    use std::net::Ipv4Addr;

    const OUR_MAC: MacAddr = [0x02, 0, 0, 0, 0, 0x01];
    const DEVICE_MAC: MacAddr = [0x02, 0, 0, 0, 0, 0x02];

    #[test]
    fn arp_frame_yields_a_sighting_with_ip() {
        let arp = ArpPacket {
            operation: ArpOperation::Request,
            sender_mac: DEVICE_MAC,
            sender_ip: Ipv4Addr::new(10, 0, 0, 5),
            target_mac: [0; 6],
            target_ip: Ipv4Addr::new(10, 0, 0, 1),
        };
        let frame = EthernetFrame {
            dst: BROADCAST_MAC,
            src: DEVICE_MAC,
            ethertype: ETHERTYPE_ARP,
            payload: arp.encode().to_vec(),
        }
        .encode();

        let event = passive_sighting_from_frame(&frame, OUR_MAC, 100).unwrap();
        match event {
            Event::Sighting {
                device_id,
                ip,
                source,
                at,
            } => {
                assert_eq!(device_id, format_mac(&DEVICE_MAC));
                assert_eq!(ip.as_deref(), Some("10.0.0.5"));
                assert_eq!(source, SightingSource::Passive);
                assert_eq!(at, 100);
            }
            other => panic!("expected a Sighting, got {other:?}"),
        }
    }

    #[test]
    fn frame_from_ourselves_is_ignored() {
        let frame = EthernetFrame {
            dst: BROADCAST_MAC,
            src: OUR_MAC,
            ethertype: ETHERTYPE_ARP,
            payload: vec![0; 28],
        }
        .encode();
        assert!(passive_sighting_from_frame(&frame, OUR_MAC, 0).is_none());
    }

    #[test]
    fn unparseable_payload_still_yields_a_mac_only_sighting() {
        let frame = EthernetFrame {
            dst: BROADCAST_MAC,
            src: DEVICE_MAC,
            ethertype: 0x86dd, // IPv6 - not parsed, but the MAC still counts
            payload: vec![0; 4],
        }
        .encode();

        let event = passive_sighting_from_frame(&frame, OUR_MAC, 0).unwrap();
        match event {
            Event::Sighting { device_id, ip, .. } => {
                assert_eq!(device_id, format_mac(&DEVICE_MAC));
                assert_eq!(ip, None);
            }
            other => panic!("expected a Sighting, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn sniffer_loop_forwards_sightings_from_the_transport() {
        let mut transport = FakeTransport::new(OUR_MAC);
        let arp = ArpPacket {
            operation: ArpOperation::Request,
            sender_mac: DEVICE_MAC,
            sender_ip: Ipv4Addr::new(10, 0, 0, 5),
            target_mac: [0; 6],
            target_ip: Ipv4Addr::new(10, 0, 0, 1),
        };
        transport.queue_incoming(
            EthernetFrame {
                dst: BROADCAST_MAC,
                src: DEVICE_MAC,
                ethertype: ETHERTYPE_ARP,
                payload: arp.encode().to_vec(),
            }
            .encode(),
        );

        let (tx, mut rx) = mpsc::channel(8);
        tokio::spawn(run_passive_sniffer(transport, tx));

        let event = rx.recv().await.unwrap();
        assert!(matches!(
            event,
            Event::Sighting {
                source: SightingSource::Passive,
                ..
            }
        ));
    }
}
