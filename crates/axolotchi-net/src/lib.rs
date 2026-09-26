//! Network scanning: ARP sweep, passive `AF_PACKET` sniffer, and a mock
//! sighting source for laptop-only development.
//!
//! Everything that needs real hardware — raw sockets, an actual LAN
//! interface — lives behind the `hardware` feature and is isolated to
//! `raw_socket.rs`. The sweep and sniffer *orchestration* (`sweep.rs`,
//! `sniffer.rs`) is written against the `Transport` trait instead, so it's
//! fully unit tested with a fake transport regardless of that feature.

pub mod arp;
pub mod ethernet;
pub mod ipv4;
pub mod mock;
#[cfg(feature = "hardware")]
pub mod raw_socket;
pub mod sniffer;
pub mod subnet;
pub mod sweep;
pub mod transport;

pub use mock::{demo_script, run_mock_source, ScriptedAction, ScriptedEvent};
#[cfg(feature = "hardware")]
pub use raw_socket::RawSocketTransport;
pub use sniffer::run_passive_sniffer;
pub use sweep::{run_arp_sweep, suspects, SweepConfig};
pub use transport::Transport;
