//! The real `Transport`: an `AF_PACKET`/`SOCK_RAW` socket bound to a live
//! interface in promiscuous mode.
//!
//! **This module needs `CAP_NET_RAW` (or root) and an actual network
//! interface. It compiles under `cross` for the Pi target but has not been
//! exercised against real hardware in this environment** — there's no
//! privileged raw socket or LAN available here to test against. Everything
//! it hands off to (`sweep::run_sweep_cycle`, `sniffer::run_passive_sniffer`)
//! is unit tested through the `Transport` trait with a fake; only the
//! syscalls in this file are unverified.
//!
//! A few `libc` doesn't expose for the plain glibc/musl Linux target (only
//! for android/l4re): `SIOCGIFINDEX` (0x8933) and `SIOCGIFHWADDR` (0x8927)
//! are long-stable Linux ioctl numbers, defined here directly rather than
//! guessing at a byte-for-byte `ifreq` struct layout — a fixed, generously
//! sized buffer is used instead, with the interface name written at offset
//! 0 and the ioctl's result read back from offset 16 (right after
//! `IFNAMSIZ`), which is where every `ifreq` union member starts.

use crate::ethernet::MacAddr;
use crate::transport::Transport;
use std::ffi::c_void;
use std::io;
use std::os::unix::io::RawFd;

const SIOCGIFINDEX: libc::Ioctl = 0x8933;
const SIOCGIFHWADDR: libc::Ioctl = 0x8927;

const IFREQ_BUF_LEN: usize = 64;
const RECV_BUF_LEN: usize = 2048;

pub struct RawSocketTransport {
    fd: RawFd,
    local_mac: MacAddr,
}

impl RawSocketTransport {
    /// Opens a raw socket on `interface`, puts it in promiscuous mode, and
    /// reads back the interface's own MAC (so callers can ignore frames
    /// Axo itself sent).
    pub fn open(interface: &str) -> io::Result<Self> {
        if interface.is_empty() || interface.len() >= libc::IFNAMSIZ {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid interface name",
            ));
        }

        // socket(2) wants the protocol in network byte order.
        let protocol = (libc::ETH_P_ALL as u16).to_be() as libc::c_int;
        let fd = unsafe { libc::socket(libc::AF_PACKET, libc::SOCK_RAW, protocol) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }

        let result = Self::setup(fd, interface, protocol);
        match result {
            Ok(local_mac) => Ok(Self { fd, local_mac }),
            Err(error) => {
                unsafe { libc::close(fd) };
                Err(error)
            }
        }
    }

    fn setup(fd: RawFd, interface: &str, protocol: libc::c_int) -> io::Result<MacAddr> {
        let ifindex = interface_index(fd, interface)?;
        let local_mac = interface_mac(fd, interface)?;

        let mut addr: libc::sockaddr_ll = unsafe { std::mem::zeroed() };
        addr.sll_family = libc::AF_PACKET as u16;
        addr.sll_protocol = protocol as u16;
        addr.sll_ifindex = ifindex;
        let ret = unsafe {
            libc::bind(
                fd,
                &addr as *const libc::sockaddr_ll as *const libc::sockaddr,
                std::mem::size_of::<libc::sockaddr_ll>() as libc::socklen_t,
            )
        };
        if ret < 0 {
            return Err(io::Error::last_os_error());
        }

        let mreq = libc::packet_mreq {
            mr_ifindex: ifindex,
            mr_type: libc::PACKET_MR_PROMISC as u16,
            mr_alen: 0,
            mr_address: [0; 8],
        };
        let ret = unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_PACKET,
                libc::PACKET_ADD_MEMBERSHIP,
                &mreq as *const libc::packet_mreq as *const c_void,
                std::mem::size_of::<libc::packet_mreq>() as libc::socklen_t,
            )
        };
        if ret < 0 {
            return Err(io::Error::last_os_error());
        }

        Ok(local_mac)
    }
}

impl Transport for RawSocketTransport {
    fn local_mac(&self) -> MacAddr {
        self.local_mac
    }

    fn send_frame(&mut self, frame: &[u8]) -> io::Result<()> {
        let ret = unsafe { libc::send(self.fd, frame.as_ptr() as *const c_void, frame.len(), 0) };
        if ret < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    fn recv_frame(&mut self) -> io::Result<Option<Vec<u8>>> {
        let mut buf = [0u8; RECV_BUF_LEN];
        let ret = unsafe {
            libc::recv(
                self.fd,
                buf.as_mut_ptr() as *mut c_void,
                buf.len(),
                libc::MSG_DONTWAIT,
            )
        };
        if ret < 0 {
            let error = io::Error::last_os_error();
            return match error.kind() {
                io::ErrorKind::WouldBlock => Ok(None),
                _ => Err(error),
            };
        }
        Ok(Some(buf[..ret as usize].to_vec()))
    }
}

impl Drop for RawSocketTransport {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.fd);
        }
    }
}

fn ifreq_buffer(interface: &str) -> [u8; IFREQ_BUF_LEN] {
    let mut buf = [0u8; IFREQ_BUF_LEN];
    buf[..interface.len()].copy_from_slice(interface.as_bytes());
    buf
}

fn ioctl_ifreq(fd: RawFd, request: libc::Ioctl, buf: &mut [u8; IFREQ_BUF_LEN]) -> io::Result<()> {
    let ret = unsafe { libc::ioctl(fd, request, buf.as_mut_ptr()) };
    if ret < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn interface_index(fd: RawFd, interface: &str) -> io::Result<libc::c_int> {
    let mut buf = ifreq_buffer(interface);
    ioctl_ifreq(fd, SIOCGIFINDEX, &mut buf)?;
    // The ifr_ifindex union member is a plain `int` starting right after
    // the 16-byte ifr_name.
    Ok(libc::c_int::from_ne_bytes(buf[16..20].try_into().unwrap()))
}

fn interface_mac(fd: RawFd, interface: &str) -> io::Result<MacAddr> {
    let mut buf = ifreq_buffer(interface);
    ioctl_ifreq(fd, SIOCGIFHWADDR, &mut buf)?;
    // ifr_hwaddr is a `struct sockaddr`: a 2-byte family field followed by
    // the address bytes, both starting at the same offset 16 as any other
    // union member.
    let mut mac = [0u8; 6];
    mac.copy_from_slice(&buf[18..24]);
    Ok(mac)
}
