//! The `Transport` trait is the seam between protocol logic and hardware.
//!
//! `run_sweep_cycle`/`run_arp_sweep` (sweep.rs) and `run_passive_sniffer`
//! (sniffer.rs) are written against this trait and generic over it, so
//! they're fully exercised in `cargo test` using `FakeTransport` below.
//! Only `raw_socket::RawSocketTransport` — the real AF_PACKET
//! implementation — needs actual hardware, and it's the one piece of this
//! crate that isn't (and can't be) unit tested here.

use crate::ethernet::MacAddr;
use std::io;

pub trait Transport {
    fn local_mac(&self) -> MacAddr;

    fn send_frame(&mut self, frame: &[u8]) -> io::Result<()>;

    /// Returns the next received frame, or `None` if nothing is available
    /// right now. Implementations must not block indefinitely.
    fn recv_frame(&mut self) -> io::Result<Option<Vec<u8>>>;
}

#[cfg(test)]
pub struct FakeTransport {
    pub local_mac: MacAddr,
    pub sent: Vec<Vec<u8>>,
    pub incoming: std::collections::VecDeque<Vec<u8>>,
}

#[cfg(test)]
impl FakeTransport {
    pub fn new(local_mac: MacAddr) -> Self {
        Self {
            local_mac,
            sent: Vec::new(),
            incoming: std::collections::VecDeque::new(),
        }
    }

    pub fn queue_incoming(&mut self, frame: Vec<u8>) {
        self.incoming.push_back(frame);
    }
}

#[cfg(test)]
impl Transport for FakeTransport {
    fn local_mac(&self) -> MacAddr {
        self.local_mac
    }

    fn send_frame(&mut self, frame: &[u8]) -> io::Result<()> {
        self.sent.push(frame.to_vec());
        Ok(())
    }

    fn recv_frame(&mut self) -> io::Result<Option<Vec<u8>>> {
        Ok(self.incoming.pop_front())
    }
}
