//! Connection request message.

use super::id;
use crate::error::{Error, Result};

/// A connection request message sent by a client to establish a connection.
#[derive(Debug, Clone, Default)]
pub struct ConnectionRequest {
    pub client_guid: u64,
    pub request_time: i64,
    pub secure: bool,
}

impl ConnectionRequest {
    /// Creates a new connection request.
    pub fn new(client_guid: u64, request_time: i64) -> Self {
        Self {
            client_guid,
            request_time,
            secure: false,
        }
    }

    /// Deserializes the message from bytes.
    pub fn read(data: &[u8]) -> Result<Self> {
        if data.len() < 17 {
            return Err(Error::UnexpectedEof);
        }
        let client_guid = u64::from_be_bytes([
            data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
        ]);
        let request_time = i64::from_be_bytes([
            data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15],
        ]);
        let secure = data[16] != 0;
        Ok(Self {
            client_guid,
            request_time,
            secure,
        })
    }

    /// Serializes the message to bytes.
    pub fn write(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(18);
        buf.push(id::CONNECTION_REQUEST);
        buf.extend_from_slice(&self.client_guid.to_be_bytes());
        buf.extend_from_slice(&self.request_time.to_be_bytes());
        buf.push(if self.secure { 1 } else { 0 });
        buf
    }
}
