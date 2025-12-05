//! Connected ping message.

use crate::error::{Error, Result};
use super::id;

/// A connected ping message sent over an established connection.
#[derive(Debug, Clone, Default)]
pub struct ConnectedPing {
    pub ping_time: i64,
}

impl ConnectedPing {
    /// Creates a new connected ping with the given timestamp.
    pub fn new(ping_time: i64) -> Self {
        Self { ping_time }
    }

    /// Deserializes the message from bytes.
    pub fn read(data: &[u8]) -> Result<Self> {
        if data.len() < 8 {
            return Err(Error::UnexpectedEof);
        }
        let ping_time = i64::from_be_bytes([
            data[0], data[1], data[2], data[3],
            data[4], data[5], data[6], data[7],
        ]);
        Ok(Self { ping_time })
    }

    /// Serializes the message to bytes.
    pub fn write(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(9);
        buf.push(id::CONNECTED_PING);
        buf.extend_from_slice(&self.ping_time.to_be_bytes());
        buf
    }
}
