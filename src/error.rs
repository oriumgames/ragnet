//! Error types for the RakNet library.

use std::net::SocketAddr;
use thiserror::Error;

/// Result type alias for RakNet operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Error type for RakNet operations.
#[derive(Error, Debug)]
pub enum Error {
    /// A message sent was larger than the buffer used to receive the message into.
    #[error("a message sent was larger than the buffer used to receive the message into")]
    BufferTooSmall,

    /// The listener has been closed.
    #[error("use of closed listener")]
    ListenerClosed,

    /// The connection has been closed.
    #[error("use of closed connection")]
    ConnectionClosed,

    /// Feature not supported.
    #[error("feature not supported")]
    NotSupported,

    /// IO error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Unexpected end of file.
    #[error("unexpected end of file")]
    UnexpectedEof,

    /// Invalid packet length.
    #[error("invalid packet length: {0}")]
    InvalidPacketLength(String),

    /// Maximum acknowledgement packets exceeded.
    #[error("maximum amount of packets in acknowledgement exceeded")]
    MaxAcknowledgement,

    /// Protocol version mismatch.
    #[error("mismatched protocol: client protocol = {client}, server protocol = {server}")]
    ProtocolMismatch { client: u8, server: u8 },

    /// Connection timeout.
    #[error("connection timed out")]
    Timeout,

    /// Queue window size too big.
    #[error("queue window size is too big ({lowest}-{highest})")]
    WindowSizeTooBig { lowest: u32, highest: u32 },

    /// Split packet error.
    #[error("split packet error: {0}")]
    SplitPacket(String),

    /// Zero packet length.
    #[error("handle packet: zero packet length")]
    ZeroPacket,

    /// Invalid cookie.
    #[error("invalid cookie '{got:x}', expected '{expected:x}'")]
    InvalidCookie { got: u32, expected: u32 },

    /// Unexpected packet.
    #[error("unexpected {packet_type} packet")]
    UnexpectedPacket { packet_type: &'static str },

    /// Unknown packet.
    #[error("unknown unconnected packet (id={id:#x}, len={len})")]
    UnknownPacket { id: u8, len: usize },

    /// Generic error with message.
    #[error("{0}")]
    Other(String),

    /// Timestamp in the future.
    #[error("timestamp is in the future")]
    TimestampInFuture,
}

/// A network operation error
#[derive(Debug)]
pub struct OpError {
    pub op: &'static str,
    pub net: &'static str,
    pub source: Option<SocketAddr>,
    pub addr: Option<SocketAddr>,
    pub err: Error,
}

impl std::fmt::Display for OpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}: {}", self.op, self.net, self.err)
    }
}

impl std::error::Error for OpError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.err)
    }
}

impl OpError {
    pub fn new(op: &'static str, err: Error) -> Self {
        OpError {
            op,
            net: "raknet",
            source: None,
            addr: None,
            err,
        }
    }

    pub fn with_addr(mut self, addr: SocketAddr) -> Self {
        self.addr = Some(addr);
        self
    }

    pub fn with_source(mut self, source: SocketAddr) -> Self {
        self.source = Some(source);
        self
    }
}
