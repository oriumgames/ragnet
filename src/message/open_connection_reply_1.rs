//! Open connection reply 1 message.

use super::{id, UNCONNECTED_MESSAGE_SEQUENCE};
use crate::error::{Error, Result};

/// Open connection reply 1 - server's response to request 1.
#[derive(Debug, Clone, Default)]
pub struct OpenConnectionReply1 {
    pub server_guid: i64,
    pub server_has_security: bool,
    pub cookie: u32,
    pub mtu: u16,
}

impl OpenConnectionReply1 {
    /// Creates a new open connection reply 1.
    pub fn new(server_guid: i64, mtu: u16) -> Self {
        Self {
            server_guid,
            server_has_security: false,
            cookie: 0,
            mtu,
        }
    }

    /// Creates a new open connection reply 1 with security.
    pub fn with_security(server_guid: i64, cookie: u32, mtu: u16) -> Self {
        Self {
            server_guid,
            server_has_security: true,
            cookie,
            mtu,
        }
    }

    /// Deserializes the message from bytes.
    pub fn read(data: &[u8]) -> Result<Self> {
        if data.len() < 27 {
            return Err(Error::UnexpectedEof);
        }
        // Magic: 16 bytes (skip)
        let server_guid = i64::from_be_bytes([
            data[16], data[17], data[18], data[19], data[20], data[21], data[22], data[23],
        ]);
        let server_has_security = data[24] != 0;

        let mut offset = 0;
        let cookie = if server_has_security {
            if data.len() < 29 {
                return Err(Error::UnexpectedEof);
            }
            offset = 4;
            u32::from_be_bytes([data[25], data[26], data[27], data[28]])
        } else {
            0
        };

        if data.len() < 27 + offset {
            return Err(Error::UnexpectedEof);
        }
        let mtu = u16::from_be_bytes([data[25 + offset], data[26 + offset]]);

        Ok(Self {
            server_guid,
            server_has_security,
            cookie,
            mtu,
        })
    }

    /// Serializes the message to bytes.
    pub fn write(&self) -> Vec<u8> {
        let offset = if self.server_has_security { 4 } else { 0 };
        let mut buf = Vec::with_capacity(28 + offset);

        buf.push(id::OPEN_CONNECTION_REPLY_1);
        buf.extend_from_slice(&UNCONNECTED_MESSAGE_SEQUENCE);
        buf.extend_from_slice(&self.server_guid.to_be_bytes());

        if self.server_has_security {
            buf.push(1);
            buf.extend_from_slice(&self.cookie.to_be_bytes());
        } else {
            buf.push(0);
        }

        buf.extend_from_slice(&self.mtu.to_be_bytes());
        buf
    }
}
