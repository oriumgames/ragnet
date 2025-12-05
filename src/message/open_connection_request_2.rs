//! Open connection request 2 message.

use std::net::SocketAddr;

use super::addr::{addr_size, put_addr, read_addr, sizeof_addr};
use super::{id, UNCONNECTED_MESSAGE_SEQUENCE};
use crate::error::{Error, Result};

/// Open connection request 2 - the second request in the connection handshake.
#[derive(Debug, Clone)]
pub struct OpenConnectionRequest2 {
    pub server_address: SocketAddr,
    pub mtu: u16,
    pub client_guid: i64,
    /// Whether the server has security enabled (affects parsing).
    pub server_has_security: bool,
    pub cookie: u32,
}

impl Default for OpenConnectionRequest2 {
    fn default() -> Self {
        Self {
            server_address: "0.0.0.0:0".parse().unwrap(),
            mtu: 0,
            client_guid: 0,
            server_has_security: false,
            cookie: 0,
        }
    }
}

impl OpenConnectionRequest2 {
    /// Creates a new open connection request 2.
    pub fn new(server_address: SocketAddr, mtu: u16, client_guid: i64) -> Self {
        Self {
            server_address,
            mtu,
            client_guid,
            server_has_security: false,
            cookie: 0,
        }
    }

    /// Creates a new open connection request 2 with security.
    pub fn with_security(
        server_address: SocketAddr,
        mtu: u16,
        client_guid: i64,
        cookie: u32,
    ) -> Self {
        Self {
            server_address,
            mtu,
            client_guid,
            server_has_security: true,
            cookie,
        }
    }

    /// Deserializes the message from bytes.
    /// Note: server_has_security must be set before calling this.
    pub fn read(&mut self, data: &[u8]) -> Result<()> {
        let cookie_offset = if self.server_has_security { 5 } else { 0 };

        if data.len() < 16 + cookie_offset {
            return Err(Error::UnexpectedEof);
        }

        // Magic: 16 bytes (skip)
        let mut offset = 16;

        if self.server_has_security {
            self.cookie = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
            offset = 16 + cookie_offset;
        }

        if data.len() < offset + addr_size(&data[offset..]) {
            return Err(Error::UnexpectedEof);
        }

        let (server_address, n) = read_addr(&data[offset..]);
        self.server_address = server_address;
        offset += n;

        if data.len() < offset + 10 {
            return Err(Error::UnexpectedEof);
        }

        self.mtu = u16::from_be_bytes([data[offset], data[offset + 1]]);
        self.client_guid = i64::from_be_bytes([
            data[offset + 2],
            data[offset + 3],
            data[offset + 4],
            data[offset + 5],
            data[offset + 6],
            data[offset + 7],
            data[offset + 8],
            data[offset + 9],
        ]);

        Ok(())
    }

    /// Serializes the message to bytes.
    pub fn write(&self) -> Vec<u8> {
        let cookie_offset = if self.server_has_security { 5 } else { 0 };
        let addr_size = sizeof_addr(&self.server_address);
        let mut buf = Vec::with_capacity(27 + addr_size + cookie_offset);

        buf.push(id::OPEN_CONNECTION_REQUEST_2);
        buf.extend_from_slice(&UNCONNECTED_MESSAGE_SEQUENCE);

        if self.server_has_security {
            buf.extend_from_slice(&self.cookie.to_be_bytes());
            buf.push(0);
        }

        let mut addr_buf = [0u8; 29];
        let len = put_addr(&mut addr_buf, self.server_address);
        buf.extend_from_slice(&addr_buf[..len]);

        buf.extend_from_slice(&self.mtu.to_be_bytes());
        buf.extend_from_slice(&self.client_guid.to_be_bytes());

        buf
    }
}
