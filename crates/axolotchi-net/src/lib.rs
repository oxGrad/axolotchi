//! Network scanning: ARP sweep, passive AF_PACKET sniffer, DHCP/mDNS parsing.
//!
//! Stub crate (milestone 1). Real scanning needs raw sockets and a live LAN,
//! so it belongs behind a feature/mock split added in milestone 4 — nothing
//! here yet touches hardware.
