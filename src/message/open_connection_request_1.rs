//! Open connection request 1 message.

use crate::error::{Error, Result};
use super::{id, UNCONNECTED_MESSAGE_SEQUENCE};

/// Open connection request 1 - the first message in the connection handshake.
#[derive(Debug, Clone, Default)]
pub struct OpenConnectionRequest1 {
    pub client_protocol: u8,
    pub mtu: u16,
}

impl OpenConnectionRequest1 {
    /// Creates a new open connection request 1.
    pub fn new(client_protocol: u8, mtu: u16) -> Self {
        Self { client_protocol, mtu }
    }

    /// Deserializes the message from bytes.
    pub fn read(data: &[u8]) -> Result<Self> {
        if data.len() < 17 {
            return Err(Error::UnexpectedEof);
        }
        // Magic: 16 bytes (skip)
        let client_protocol = data[16];
        // MTU is calculated from the packet size
        let mtu = (data.len() + 20 + 8 + 1) as u16; // Headers + packet ID
        
        Ok(Self { client_protocol, mtu })
    }

    /// Serializes the message to bytes.
    pub fn write(&self) -> Vec<u8> {
        // The packet is padded to the MTU size minus IP and UDP headers
        let size = (self.mtu - 20 - 8) as usize;
        let mut buf = vec![0u8; size];
        
        buf[0] = id::OPEN_CONNECTION_REQUEST_1;
        buf[1..17].copy_from_slice(&UNCONNECTED_MESSAGE_SEQUENCE);
        buf[17] = self.client_protocol;
        // Rest is padding (already zeroed)
        
        buf
    }
}
