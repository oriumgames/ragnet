//! Connected pong message.

use crate::error::{Error, Result};
use super::id;

/// A connected pong message sent in response to a connected ping.
#[derive(Debug, Clone, Default)]
pub struct ConnectedPong {
    pub ping_time: i64,
    pub pong_time: i64,
}

impl ConnectedPong {
    /// Creates a new connected pong.
    pub fn new(ping_time: i64, pong_time: i64) -> Self {
        Self { ping_time, pong_time }
    }

    /// Deserializes the message from bytes.
    pub fn read(data: &[u8]) -> Result<Self> {
        if data.len() < 16 {
            return Err(Error::UnexpectedEof);
        }
        let ping_time = i64::from_be_bytes([
            data[0], data[1], data[2], data[3],
            data[4], data[5], data[6], data[7],
        ]);
        let pong_time = i64::from_be_bytes([
            data[8], data[9], data[10], data[11],
            data[12], data[13], data[14], data[15],
        ]);
        Ok(Self { ping_time, pong_time })
    }

    /// Serializes the message to bytes.
    pub fn write(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(17);
        buf.push(id::CONNECTED_PONG);
        buf.extend_from_slice(&self.ping_time.to_be_bytes());
        buf.extend_from_slice(&self.pong_time.to_be_bytes());
        buf
    }
}
