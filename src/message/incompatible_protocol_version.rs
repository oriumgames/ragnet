//! Incompatible protocol version message.

use crate::error::{Error, Result};
use super::{id, UNCONNECTED_MESSAGE_SEQUENCE};

/// An incompatible protocol version message sent when version mismatch occurs.
#[derive(Debug, Clone, Default)]
pub struct IncompatibleProtocolVersion {
    pub server_protocol: u8,
    pub server_guid: i64,
}

impl IncompatibleProtocolVersion {
    /// Creates a new incompatible protocol version message.
    pub fn new(server_protocol: u8, server_guid: i64) -> Self {
        Self { server_protocol, server_guid }
    }

    /// Deserializes the message from bytes.
    pub fn read(data: &[u8]) -> Result<Self> {
        if data.len() < 25 {
            return Err(Error::UnexpectedEof);
        }
        let server_protocol = data[0];
        // Magic: 16 bytes (1..17)
        let server_guid = i64::from_be_bytes([
            data[17], data[18], data[19], data[20],
            data[21], data[22], data[23], data[24],
        ]);
        Ok(Self { server_protocol, server_guid })
    }

    /// Serializes the message to bytes.
    pub fn write(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(26);
        buf.push(id::INCOMPATIBLE_PROTOCOL_VERSION);
        buf.push(self.server_protocol);
        buf.extend_from_slice(&UNCONNECTED_MESSAGE_SEQUENCE);
        buf.extend_from_slice(&self.server_guid.to_be_bytes());
        buf
    }
}
