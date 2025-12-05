//! RakNet message types for connection establishment and maintenance.

mod addr;
mod connected_ping;
mod connected_pong;
mod connection_request;
mod connection_request_accepted;
mod incompatible_protocol_version;
mod new_incoming_connection;
mod open_connection_reply_1;
mod open_connection_reply_2;
mod open_connection_request_1;
mod open_connection_request_2;
mod unconnected_ping;
mod unconnected_pong;

pub use addr::*;
pub use connected_ping::*;
pub use connected_pong::*;
pub use connection_request::*;
pub use connection_request_accepted::*;
pub use incompatible_protocol_version::*;
pub use new_incoming_connection::*;
pub use open_connection_reply_1::*;
pub use open_connection_reply_2::*;
pub use open_connection_request_1::*;
pub use open_connection_request_2::*;
pub use unconnected_ping::*;
pub use unconnected_pong::*;

/// Message IDs for RakNet protocol.
pub mod id {
    pub const CONNECTED_PING: u8 = 0x00;
    pub const UNCONNECTED_PING: u8 = 0x01;
    pub const UNCONNECTED_PING_OPEN_CONNECTIONS: u8 = 0x02;
    pub const CONNECTED_PONG: u8 = 0x03;
    pub const DETECT_LOST_CONNECTIONS: u8 = 0x04;
    pub const OPEN_CONNECTION_REQUEST_1: u8 = 0x05;
    pub const OPEN_CONNECTION_REPLY_1: u8 = 0x06;
    pub const OPEN_CONNECTION_REQUEST_2: u8 = 0x07;
    pub const OPEN_CONNECTION_REPLY_2: u8 = 0x08;
    pub const CONNECTION_REQUEST: u8 = 0x09;
    pub const CONNECTION_REQUEST_ACCEPTED: u8 = 0x10;
    pub const NEW_INCOMING_CONNECTION: u8 = 0x13;
    pub const DISCONNECT_NOTIFICATION: u8 = 0x15;
    pub const INCOMPATIBLE_PROTOCOL_VERSION: u8 = 0x19;
    pub const UNCONNECTED_PONG: u8 = 0x1c;
}

/// The magic sequence found in every unconnected RakNet message.
pub const UNCONNECTED_MESSAGE_SEQUENCE: [u8; 16] = [
    0x00, 0xff, 0xff, 0x00, 0xfe, 0xfe, 0xfe, 0xfe,
    0xfd, 0xfd, 0xfd, 0xfd, 0x12, 0x34, 0x56, 0x78,
];
