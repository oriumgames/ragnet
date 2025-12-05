//! Unconnected pong message.

use crate::error::{Error, Result};
use super::{id, UNCONNECTED_MESSAGE_SEQUENCE};

/// An unconnected pong message in response to a ping.
#[derive(Debug, Clone, Default)]
pub struct UnconnectedPong {
    pub ping_time: i64,
    pub server_guid: i64,
    pub data: Vec<u8>,
}

impl UnconnectedPong {
    /// Creates a new unconnected pong.
    pub fn new(ping_time: i64, server_guid: i64, data: Vec<u8>) -> Self {
        Self { ping_time, server_guid, data }
    }

    /// Deserializes the message from bytes.
    pub fn read(data: &[u8]) -> Result<Self> {
        if data.len() < 34 {
            return Err(Error::UnexpectedEof);
        }
        
        let ping_time = i64::from_be_bytes([
            data[0], data[1], data[2], data[3],
            data[4], data[5], data[6], data[7],
        ]);
        let server_guid = i64::from_be_bytes([
            data[8], data[9], data[10], data[11],
            data[12], data[13], data[14], data[15],
        ]);
        // Magic: 16 bytes (16..32)
        let data_len = u16::from_be_bytes([data[32], data[33]]) as usize;
        
        if data.len() < 34 + data_len {
            return Err(Error::UnexpectedEof);
        }
        
        let pong_data = data[34..34 + data_len].to_vec();
        
        Ok(Self {
            ping_time,
            server_guid,
            data: pong_data,
        })
    }

    /// Serializes the message to bytes.
    pub fn write(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(35 + self.data.len());
        buf.push(id::UNCONNECTED_PONG);
        buf.extend_from_slice(&self.ping_time.to_be_bytes());
        buf.extend_from_slice(&self.server_guid.to_be_bytes());
        buf.extend_from_slice(&UNCONNECTED_MESSAGE_SEQUENCE);
        buf.extend_from_slice(&(self.data.len() as u16).to_be_bytes());
        buf.extend_from_slice(&self.data);
        buf
    }
}
