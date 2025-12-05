//! New incoming connection message.

use std::net::SocketAddr;

use super::addr::{
    addr_size, default_system_addresses, put_addr, read_addr, sizeof_addr, sizeof_system_addresses,
    SystemAddresses,
};
use super::id;
use crate::error::{Error, Result};

/// A new incoming connection message sent by the client after connection acceptance.
#[derive(Debug, Clone)]
pub struct NewIncomingConnection {
    pub server_address: SocketAddr,
    pub system_addresses: SystemAddresses,
    pub ping_time: i64,
    pub pong_time: i64,
}

impl Default for NewIncomingConnection {
    fn default() -> Self {
        Self {
            server_address: "0.0.0.0:0".parse().unwrap(),
            system_addresses: default_system_addresses(),
            ping_time: 0,
            pong_time: 0,
        }
    }
}

impl NewIncomingConnection {
    /// Creates a new incoming connection message.
    pub fn new(server_address: SocketAddr, ping_time: i64, pong_time: i64) -> Self {
        Self {
            server_address,
            system_addresses: default_system_addresses(),
            ping_time,
            pong_time,
        }
    }

    /// Deserializes the message from bytes.
    pub fn read(data: &[u8]) -> Result<Self> {
        if data.len() < addr_size(data) {
            return Err(Error::UnexpectedEof);
        }

        let (server_address, mut offset) = read_addr(data);

        let mut system_addresses = default_system_addresses();
        for system_addr in &mut system_addresses {
            if data.len() - offset == 16 {
                // Some implementations send only 10 system addresses
                break;
            }
            if data.len() - offset < addr_size(&data[offset..]) {
                return Err(Error::UnexpectedEof);
            }
            let (addr, n) = read_addr(&data[offset..]);
            *system_addr = addr;
            offset += n;
        }

        if data.len() - offset < 16 {
            return Err(Error::UnexpectedEof);
        }
        let ping_time = i64::from_be_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
            data[offset + 4],
            data[offset + 5],
            data[offset + 6],
            data[offset + 7],
        ]);
        let pong_time = i64::from_be_bytes([
            data[offset + 8],
            data[offset + 9],
            data[offset + 10],
            data[offset + 11],
            data[offset + 12],
            data[offset + 13],
            data[offset + 14],
            data[offset + 15],
        ]);

        Ok(Self {
            server_address,
            system_addresses,
            ping_time,
            pong_time,
        })
    }

    /// Serializes the message to bytes.
    pub fn write(&self) -> Vec<u8> {
        let n_addr = sizeof_addr(&self.server_address);
        let n_sys = sizeof_system_addresses(&self.system_addresses);
        let mut buf = Vec::with_capacity(1 + n_addr + n_sys + 16);

        buf.push(id::NEW_INCOMING_CONNECTION);

        // Server address
        let mut addr_buf = [0u8; 29]; // Max size for IPv6
        let addr_len = put_addr(&mut addr_buf, self.server_address);
        buf.extend_from_slice(&addr_buf[..addr_len]);

        // System addresses
        for addr in &self.system_addresses {
            let len = put_addr(&mut addr_buf, *addr);
            buf.extend_from_slice(&addr_buf[..len]);
        }

        // Timestamps
        buf.extend_from_slice(&self.ping_time.to_be_bytes());
        buf.extend_from_slice(&self.pong_time.to_be_bytes());

        buf
    }
}
