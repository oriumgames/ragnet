//! Connection request accepted message.

use std::net::SocketAddr;

use super::addr::{
    addr_size, default_system_addresses, put_addr, read_addr, sizeof_addr, sizeof_system_addresses,
    SystemAddresses,
};
use super::id;
use crate::error::{Error, Result};

/// A connection request accepted message sent by the server.
#[derive(Debug, Clone)]
pub struct ConnectionRequestAccepted {
    pub client_address: SocketAddr,
    pub system_index: u16,
    pub system_addresses: SystemAddresses,
    pub ping_time: i64,
    pub pong_time: i64,
}

impl Default for ConnectionRequestAccepted {
    fn default() -> Self {
        Self {
            client_address: "0.0.0.0:0".parse().unwrap(),
            system_index: 0,
            system_addresses: default_system_addresses(),
            ping_time: 0,
            pong_time: 0,
        }
    }
}

impl ConnectionRequestAccepted {
    /// Creates a new connection request accepted message.
    pub fn new(client_address: SocketAddr, ping_time: i64, pong_time: i64) -> Self {
        Self {
            client_address,
            system_index: 0,
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

        let (client_address, mut offset) = read_addr(data);

        if data.len() < offset + 2 {
            return Err(Error::UnexpectedEof);
        }
        let system_index = u16::from_be_bytes([data[offset], data[offset + 1]]);
        offset += 2;

        let mut system_addresses = default_system_addresses();
        for system_addr in &mut system_addresses {
            if data.len() - offset == 16 {
                // Some implementations send fewer system addresses
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
            client_address,
            system_index,
            system_addresses,
            ping_time,
            pong_time,
        })
    }

    /// Serializes the message to bytes.
    pub fn write(&self) -> Vec<u8> {
        let n_addr = sizeof_addr(&self.client_address);
        let n_sys = sizeof_system_addresses(&self.system_addresses);
        let mut buf = Vec::with_capacity(1 + n_addr + 2 + n_sys + 16);

        buf.push(id::CONNECTION_REQUEST_ACCEPTED);

        // Client address
        let mut addr_buf = [0u8; 29]; // Max size for IPv6
        let addr_len = put_addr(&mut addr_buf, self.client_address);
        buf.extend_from_slice(&addr_buf[..addr_len]);

        // System index
        buf.extend_from_slice(&self.system_index.to_be_bytes());

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
