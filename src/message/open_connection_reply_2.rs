//! Open connection reply 2 message.

use std::net::SocketAddr;

use crate::error::{Error, Result};
use super::addr::{addr_size, put_addr, read_addr, sizeof_addr};
use super::{id, UNCONNECTED_MESSAGE_SEQUENCE};

/// Open connection reply 2 - server's final response completing the handshake.
#[derive(Debug, Clone)]
pub struct OpenConnectionReply2 {
    pub server_guid: i64,
    pub client_address: SocketAddr,
    pub mtu: u16,
    pub do_security: bool,
}

impl Default for OpenConnectionReply2 {
    fn default() -> Self {
        Self {
            server_guid: 0,
            client_address: "0.0.0.0:0".parse().unwrap(),
            mtu: 0,
            do_security: false,
        }
    }
}

impl OpenConnectionReply2 {
    /// Creates a new open connection reply 2.
    pub fn new(server_guid: i64, client_address: SocketAddr, mtu: u16) -> Self {
        Self {
            server_guid,
            client_address,
            mtu,
            do_security: false,
        }
    }

    /// Deserializes the message from bytes.
    pub fn read(data: &[u8]) -> Result<Self> {
        if data.len() < 24 {
            return Err(Error::UnexpectedEof);
        }
        
        // Magic: 16 bytes (skip)
        let server_guid = i64::from_be_bytes([
            data[16], data[17], data[18], data[19],
            data[20], data[21], data[22], data[23],
        ]);
        
        if data.len() < 24 + addr_size(&data[24..]) {
            return Err(Error::UnexpectedEof);
        }
        
        let (client_address, offset) = read_addr(&data[24..]);
        
        if data.len() < 24 + offset + 3 {
            return Err(Error::UnexpectedEof);
        }
        
        let mtu = u16::from_be_bytes([data[24 + offset], data[25 + offset]]);
        let do_security = data[26 + offset] != 0;

        Ok(Self {
            server_guid,
            client_address,
            mtu,
            do_security,
        })
    }

    /// Serializes the message to bytes.
    pub fn write(&self) -> Vec<u8> {
        let addr_size = sizeof_addr(&self.client_address);
        let mut buf = Vec::with_capacity(28 + addr_size);

        buf.push(id::OPEN_CONNECTION_REPLY_2);
        buf.extend_from_slice(&UNCONNECTED_MESSAGE_SEQUENCE);
        buf.extend_from_slice(&self.server_guid.to_be_bytes());
        
        let mut addr_buf = [0u8; 29];
        let len = put_addr(&mut addr_buf, self.client_address);
        buf.extend_from_slice(&addr_buf[..len]);
        
        buf.extend_from_slice(&self.mtu.to_be_bytes());
        buf.push(if self.do_security { 1 } else { 0 });

        buf
    }
}
