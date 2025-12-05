//! Unconnected ping message.

use crate::error::{Error, Result};
use super::{id, UNCONNECTED_MESSAGE_SEQUENCE};

/// An unconnected ping message for server discovery.
#[derive(Debug, Clone, Default)]
pub struct UnconnectedPing {
    pub ping_time: i64,
    pub client_guid: i64,
}

impl UnconnectedPing {
    /// Creates a new unconnected ping.
    pub fn new(ping_time: i64, client_guid: i64) -> Self {
        Self { ping_time, client_guid }
    }

    /// Deserializes the message from bytes.
    pub fn read(data: &[u8]) -> Result<Self> {
        if data.len() < 32 {
            return Err(Error::UnexpectedEof);
        }
        let ping_time = i64::from_be_bytes([
            data[0], data[1], data[2], data[3],
            data[4], data[5], data[6], data[7],
        ]);
        // Magic: 16 bytes (8..24)
        let client_guid = i64::from_be_bytes([
            data[24], data[25], data[26], data[27],
            data[28], data[29], data[30], data[31],
        ]);
        Ok(Self { ping_time, client_guid })
    }

    /// Serializes the message to bytes.
    pub fn write(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(33);
        buf.push(id::UNCONNECTED_PING);
        buf.extend_from_slice(&self.ping_time.to_be_bytes());
        buf.extend_from_slice(&UNCONNECTED_MESSAGE_SEQUENCE);
        buf.extend_from_slice(&self.client_guid.to_be_bytes());
        buf
    }
}
