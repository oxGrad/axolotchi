//! Active ARP sweeping: broadcast-probe the subnet, then unicast-retry any
//! previously seen device that didn't answer (`suspects`) before conceding
//! a miss for this cycle. Generic over `Transport`, so it's fully testable
//! with a fake — only `raw_socket::RawSocketTransport` needs real hardware.

use crate::arp::{ArpOperation, ArpPacket};
use crate::ethernet::{
    format_mac, parse_mac, EthernetFrame, MacAddr, BROADCAST_MAC, ETHERTYPE_ARP,
};
use crate::subnet;
use crate::transport::Transport;
use axolotchi_core::{DeviceId, Event, SightingSource};
use std::collections::{HashMap, HashSet};
use std::net::Ipv4Addr;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::Instant;

#[derive(Debug, Clone)]
pub struct SweepConfig {
    pub our_ip: Ipv4Addr,
    pub network: Ipv4Addr,
    pub prefix_len: u8,
    /// How long to wait for ARP replies after each probing pass.
    pub reply_window: Duration,
    /// How long to sleep between sweep cycles.
    pub interval: Duration,
}

/// Picks previously seen devices that weren't confirmed by this sweep's
/// broadcast pass, so they can get one more direct, unicast probe before
/// we concede a miss for this cycle. Pure — no socket involved.
pub fn suspects<'a>(
    known_ips: &'a HashMap<DeviceId, Ipv4Addr>,
    confirmed: &HashSet<DeviceId>,
) -> Vec<(&'a DeviceId, Ipv4Addr)> {
    known_ips
        .iter()
        .filter(|(id, _)| !confirmed.contains(*id))
        .map(|(id, ip)| (id, *ip))
        .collect()
}

fn send_arp_request<T: Transport>(
    transport: &mut T,
    our_mac: MacAddr,
    our_ip: Ipv4Addr,
    dst_mac: MacAddr,
    target_ip: Ipv4Addr,
) -> std::io::Result<()> {
    let arp = ArpPacket {
        operation: ArpOperation::Request,
        sender_mac: our_mac,
        sender_ip: our_ip,
        target_mac: [0; 6],
        target_ip,
    };
    let frame = EthernetFrame {
        dst: dst_mac,
        src: our_mac,
        ethertype: ETHERTYPE_ARP,
        payload: arp.encode().to_vec(),
    };
    transport.send_frame(&frame.encode())
}

async fn collect_replies<T: Transport>(
    transport: &mut T,
    our_mac: MacAddr,
    window: Duration,
    confirmed: &mut HashMap<DeviceId, Ipv4Addr>,
) -> std::io::Result<()> {
    let deadline = Instant::now() + window;
    while Instant::now() < deadline {
        match transport.recv_frame()? {
            Some(bytes) => {
                let Some(frame) = EthernetFrame::decode(&bytes) else {
                    continue;
                };
                if frame.src == our_mac || frame.ethertype != ETHERTYPE_ARP {
                    continue;
                }
                if let Some(arp) = ArpPacket::decode(&frame.payload) {
                    if arp.operation == ArpOperation::Reply {
                        confirmed.insert(format_mac(&arp.sender_mac), arp.sender_ip);
                    }
                }
            }
            None => tokio::time::sleep(Duration::from_millis(5)).await,
        }
    }
    Ok(())
}

/// Runs one full sweep: broadcast-probe every host in the configured
/// subnet, unicast-retry any known device the broadcast pass missed, then
/// report every confirmed device plus a `SweepComplete`. `known_ips` is
/// this sweep's own memory of "last IP seen for this device", carried
/// across cycles by the caller — it's how `suspects` knows who to retry.
pub async fn run_sweep_cycle<T: Transport>(
    transport: &mut T,
    config: &SweepConfig,
    known_ips: &mut HashMap<DeviceId, Ipv4Addr>,
    tx: &mpsc::Sender<Event>,
    now: impl Fn() -> i64,
) -> std::io::Result<()> {
    let our_mac = transport.local_mac();
    let mut confirmed: HashMap<DeviceId, Ipv4Addr> = HashMap::new();

    for target_ip in subnet::hosts(config.network, config.prefix_len).unwrap_or_default() {
        if target_ip == config.our_ip {
            continue;
        }
        send_arp_request(transport, our_mac, config.our_ip, BROADCAST_MAC, target_ip)?;
    }
    collect_replies(transport, our_mac, config.reply_window, &mut confirmed).await?;

    let confirmed_ids: HashSet<DeviceId> = confirmed.keys().cloned().collect();
    for (device_id, target_ip) in suspects(known_ips, &confirmed_ids) {
        if let Some(mac) = parse_mac(device_id) {
            send_arp_request(transport, our_mac, config.our_ip, mac, target_ip)?;
        }
    }
    collect_replies(transport, our_mac, config.reply_window, &mut confirmed).await?;

    known_ips.extend(confirmed.iter().map(|(id, ip)| (id.clone(), *ip)));

    for (device_id, ip) in &confirmed {
        let event = Event::Sighting {
            device_id: device_id.clone(),
            ip: Some(ip.to_string()),
            source: SightingSource::Active,
            vendor: None,
            at: now(),
        };
        if tx.send(event).await.is_err() {
            return Ok(());
        }
    }
    let _ = tx.send(Event::SweepComplete { at: now() }).await;
    Ok(())
}

fn unix_now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Runs sweep cycles forever, `config.interval` apart.
pub async fn run_arp_sweep<T: Transport>(
    mut transport: T,
    config: SweepConfig,
    tx: mpsc::Sender<Event>,
) {
    let mut known_ips: HashMap<DeviceId, Ipv4Addr> = HashMap::new();
    loop {
        if let Err(error) =
            run_sweep_cycle(&mut transport, &config, &mut known_ips, &tx, unix_now).await
        {
            tracing::warn!(%error, "arp sweep cycle failed");
        }
        tokio::time::sleep(config.interval).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::FakeTransport;

    const OUR_MAC: MacAddr = [0x02, 0, 0, 0, 0, 0x01];

    fn arp_reply_frame(dst: MacAddr, sender_mac: MacAddr, sender_ip: Ipv4Addr) -> Vec<u8> {
        let arp = ArpPacket {
            operation: ArpOperation::Reply,
            sender_mac,
            sender_ip,
            target_mac: OUR_MAC,
            target_ip: Ipv4Addr::new(192, 168, 1, 1),
        };
        EthernetFrame {
            dst,
            src: sender_mac,
            ethertype: ETHERTYPE_ARP,
            payload: arp.encode().to_vec(),
        }
        .encode()
    }

    fn config() -> SweepConfig {
        SweepConfig {
            our_ip: Ipv4Addr::new(192, 168, 1, 1),
            network: Ipv4Addr::new(192, 168, 1, 0),
            prefix_len: 30, // .1 and .2 only, keeps the test fast
            reply_window: Duration::from_millis(20),
            interval: Duration::from_secs(1),
        }
    }

    #[tokio::test]
    async fn broadcast_pass_probes_every_host_in_the_subnet() {
        let mut transport = FakeTransport::new(OUR_MAC);
        let (tx, _rx) = mpsc::channel(8);
        let mut known_ips = HashMap::new();
        run_sweep_cycle(&mut transport, &config(), &mut known_ips, &tx, || 0)
            .await
            .unwrap();

        // /30 rooted at .1 yields exactly one other host, .2.
        assert_eq!(transport.sent.len(), 1);
        let frame = EthernetFrame::decode(&transport.sent[0]).unwrap();
        assert_eq!(frame.dst, BROADCAST_MAC);
        let arp = ArpPacket::decode(&frame.payload).unwrap();
        assert_eq!(arp.target_ip, Ipv4Addr::new(192, 168, 1, 2));
    }

    #[tokio::test]
    async fn a_reply_produces_a_sighting_and_sweep_complete() {
        let mut transport = FakeTransport::new(OUR_MAC);
        let device_mac = [0x02, 0, 0, 0, 0, 0x02];
        transport.queue_incoming(arp_reply_frame(
            OUR_MAC,
            device_mac,
            Ipv4Addr::new(192, 168, 1, 2),
        ));

        let (tx, mut rx) = mpsc::channel(8);
        let mut known_ips = HashMap::new();
        run_sweep_cycle(&mut transport, &config(), &mut known_ips, &tx, || 42)
            .await
            .unwrap();
        drop(tx);

        let sighting = rx.recv().await.unwrap();
        match sighting {
            Event::Sighting {
                device_id,
                ip,
                source,
                at,
                ..
            } => {
                assert_eq!(device_id, format_mac(&device_mac));
                assert_eq!(ip.as_deref(), Some("192.168.1.2"));
                assert_eq!(source, SightingSource::Active);
                assert_eq!(at, 42);
            }
            other => panic!("expected a Sighting, got {other:?}"),
        }
        assert!(matches!(rx.recv().await, Some(Event::SweepComplete { .. })));
    }

    #[tokio::test]
    async fn a_device_missed_by_broadcast_gets_a_unicast_retry() {
        let mut transport = FakeTransport::new(OUR_MAC);
        let mut known_ips = HashMap::new();
        known_ips.insert(
            "02:00:00:00:00:02".to_string(),
            Ipv4Addr::new(192, 168, 1, 2),
        );

        let (tx, _rx) = mpsc::channel(8);
        run_sweep_cycle(&mut transport, &config(), &mut known_ips, &tx, || 0)
            .await
            .unwrap();

        // One broadcast probe (.2) plus one unicast retry addressed
        // directly to the suspect's own MAC.
        assert_eq!(transport.sent.len(), 2);
        let retry = EthernetFrame::decode(&transport.sent[1]).unwrap();
        assert_eq!(retry.dst, [0x02, 0, 0, 0, 0, 0x02]);
    }

    #[tokio::test]
    async fn a_device_confirmed_by_broadcast_is_not_retried() {
        let mut transport = FakeTransport::new(OUR_MAC);
        let device_mac = [0x02, 0, 0, 0, 0, 0x02];
        transport.queue_incoming(arp_reply_frame(
            OUR_MAC,
            device_mac,
            Ipv4Addr::new(192, 168, 1, 2),
        ));

        let mut known_ips = HashMap::new();
        known_ips.insert(format_mac(&device_mac), Ipv4Addr::new(192, 168, 1, 2));

        let (tx, _rx) = mpsc::channel(8);
        run_sweep_cycle(&mut transport, &config(), &mut known_ips, &tx, || 0)
            .await
            .unwrap();

        // Just the one broadcast probe — no unicast retry needed.
        assert_eq!(transport.sent.len(), 1);
    }

    #[test]
    fn suspects_excludes_confirmed_and_ip_unknown_devices() {
        let mut known_ips = HashMap::new();
        known_ips.insert("dev-1".to_string(), Ipv4Addr::new(10, 0, 0, 1));
        known_ips.insert("dev-2".to_string(), Ipv4Addr::new(10, 0, 0, 2));

        let mut confirmed = HashSet::new();
        confirmed.insert("dev-1".to_string());

        let result = suspects(&known_ips, &confirmed);
        assert_eq!(
            result,
            vec![(&"dev-2".to_string(), Ipv4Addr::new(10, 0, 0, 2))]
        );
    }
}
