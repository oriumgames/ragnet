//! Address serialization for RakNet messages.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

/// Size of an IPv4 address in RakNet format.
pub const SIZEOF_ADDR4: usize = 1 + 4 + 2;

/// Size of an IPv6 address in RakNet format.
pub const SIZEOF_ADDR6: usize = 1 + 2 + 2 + 4 + 16 + 4;

/// System addresses array (20 addresses).
pub type SystemAddresses = [SocketAddr; 20];

/// Returns the default system addresses (all zeros).
pub fn default_system_addresses() -> SystemAddresses {
    [SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0); 20]
}

/// Returns the size of an address in bytes.
pub fn sizeof_addr(addr: &SocketAddr) -> usize {
    match addr.ip() {
        IpAddr::V4(_) => SIZEOF_ADDR4,
        IpAddr::V6(_) => SIZEOF_ADDR6,
    }
}

/// Returns the total size of system addresses in bytes.
pub fn sizeof_system_addresses(addresses: &SystemAddresses) -> usize {
    addresses.iter().map(sizeof_addr).sum()
}

/// Writes an address to the buffer and returns the number of bytes written.
pub fn put_addr(buf: &mut [u8], addr: SocketAddr) -> usize {
    match addr.ip() {
        IpAddr::V4(ip) => {
            if ip.is_unspecified() {
                buf[0] = 4;
                buf[1] = 255;
                buf[2] = 255;
                buf[3] = 255;
                buf[4] = 255;
            } else {
                let octets = ip.octets();
                buf[0] = 4;
                buf[1] = !octets[0];
                buf[2] = !octets[1];
                buf[3] = !octets[2];
                buf[4] = !octets[3];
            }
            buf[5] = (addr.port() >> 8) as u8;
            buf[6] = addr.port() as u8;
            SIZEOF_ADDR4
        }
        IpAddr::V6(ip) => {
            let octets = ip.octets();
            buf[0] = 6;
            // AF_INET6 on Windows (23)
            buf[1] = 23;
            buf[2] = 0;
            // Port (big-endian)
            buf[3] = (addr.port() >> 8) as u8;
            buf[4] = addr.port() as u8;
            // Flow info (4 bytes)
            buf[5] = 0;
            buf[6] = 0;
            buf[7] = 0;
            buf[8] = 0;
            // Address (16 bytes)
            buf[9..25].copy_from_slice(&octets);
            // Scope ID (4 bytes)
            buf[25] = 0;
            buf[26] = 0;
            buf[27] = 0;
            buf[28] = 0;
            SIZEOF_ADDR6
        }
    }
}

/// Reads an address from the buffer and returns it along with the number of bytes read.
pub fn read_addr(buf: &[u8]) -> (SocketAddr, usize) {
    if buf.is_empty() || buf[0] == 4 || buf[0] == 0 {
        // IPv4
        let ip = Ipv4Addr::new(
            (-(buf[1] as i16) - 1) as u8,
            (-(buf[2] as i16) - 1) as u8,
            (-(buf[3] as i16) - 1) as u8,
            (-(buf[4] as i16) - 1) as u8,
        );
        let port = u16::from_be_bytes([buf[5], buf[6]]);
        (SocketAddr::new(IpAddr::V4(ip), port), SIZEOF_ADDR4)
    } else {
        // IPv6
        let port = u16::from_be_bytes([buf[3], buf[4]]);
        let mut octets = [0u8; 16];
        octets.copy_from_slice(&buf[9..25]);
        let ip = Ipv6Addr::from(octets);
        (SocketAddr::new(IpAddr::V6(ip), port), SIZEOF_ADDR6)
    }
}

/// Returns the size of an address from the buffer.
pub fn addr_size(buf: &[u8]) -> usize {
    if buf.is_empty() || buf[0] == 4 || buf[0] == 0 {
        SIZEOF_ADDR4
    } else {
        SIZEOF_ADDR6
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipv4_roundtrip() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 19132);
        let mut buf = [0u8; SIZEOF_ADDR4];
        let n = put_addr(&mut buf, addr);
        assert_eq!(n, SIZEOF_ADDR4);

        let (read_addr, read_n) = read_addr(&buf);
        assert_eq!(read_n, SIZEOF_ADDR4);
        assert_eq!(read_addr, addr);
    }

    #[test]
    fn test_ipv6_roundtrip() {
        let addr = SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 19132);
        let mut buf = [0u8; SIZEOF_ADDR6];
        let n = put_addr(&mut buf, addr);
        assert_eq!(n, SIZEOF_ADDR6);

        let (read_addr, read_n) = read_addr(&buf);
        assert_eq!(read_n, SIZEOF_ADDR6);
        assert_eq!(read_addr.port(), addr.port());
    }
}
