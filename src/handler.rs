//! Connection handlers for listener and dialer connections.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use parking_lot::RwLock;

use crate::conn::Conn;
use crate::error::{Error, Result};
use crate::message::{
    id, ConnectedPing, ConnectedPong, ConnectionRequest, ConnectionRequestAccepted,
    NewIncomingConnection,
};
use crate::timestamp;

/// Trait for handling connection-specific operations.
pub trait ConnectionHandler: Send + Sync + 'static {
    /// Handles a packet and returns whether it was handled internally.
    fn handle(&self, conn: &Conn, data: &[u8]) -> Result<bool>;

    /// Returns whether connection limits are enabled.
    fn limits_enabled(&self) -> bool;

    /// Called when the connection is closed.
    fn close(&self, conn: &Conn);
}

/// Handler for server-side (listener) connections.
pub struct ListenerConnectionHandler {
    pub(crate) cookie_salt: u32,
    pub(crate) disable_cookies: bool,
    /// Reference to the listener's connections map for cleanup on close.
    pub(crate) connections: Arc<RwLock<HashMap<SocketAddr, Arc<Conn>>>>,
}

impl ListenerConnectionHandler {
    /// Creates a new listener connection handler.
    pub fn new(
        cookie_salt: u32,
        disable_cookies: bool,
        connections: Arc<RwLock<HashMap<SocketAddr, Arc<Conn>>>>,
    ) -> Self {
        Self {
            cookie_salt,
            disable_cookies,
            connections,
        }
    }

    /// Calculates a cookie for the given address.
    pub fn cookie(&self, addr: SocketAddr) -> u32 {
        if self.disable_cookies {
            return 0;
        }

        let mut data = vec![0u8; 6];
        data[0..4].copy_from_slice(&self.cookie_salt.to_le_bytes());
        data[0..2].copy_from_slice(&addr.port().to_le_bytes()); // Overwrites first 2 bytes of salt

        match addr.ip() {
            std::net::IpAddr::V4(ip) => data.extend_from_slice(&ip.octets()),
            std::net::IpAddr::V6(ip) => data.extend_from_slice(&ip.octets()),
        }

        crc32fast::hash(&data)
    }

    /// Handles a connection request packet.
    pub fn handle_connection_request(&self, conn: &Conn, data: &[u8]) -> Result<()> {
        let pk = ConnectionRequest::read(data)?;
        let response =
            ConnectionRequestAccepted::new(conn.remote_addr(), pk.request_time, timestamp());
        conn.send_message(&response.write())
    }

    /// Handles a new incoming connection packet.
    pub fn handle_new_incoming_connection(&self, conn: &Conn) -> Result<()> {
        conn.mark_connected();
        Ok(())
    }
}

impl ConnectionHandler for ListenerConnectionHandler {
    fn handle(&self, conn: &Conn, data: &[u8]) -> Result<bool> {
        if data.is_empty() {
            return Ok(false);
        }

        match data[0] {
            id::CONNECTION_REQUEST => {
                self.handle_connection_request(conn, &data[1..])?;
                Ok(true)
            }
            id::CONNECTION_REQUEST_ACCEPTED => Err(Error::UnexpectedPacket {
                packet_type: "CONNECTION_REQUEST_ACCEPTED",
            }),
            id::NEW_INCOMING_CONNECTION => {
                self.handle_new_incoming_connection(conn)?;
                Ok(true)
            }
            id::CONNECTED_PING => {
                handle_connected_ping(conn, &data[1..])?;
                Ok(true)
            }
            id::CONNECTED_PONG => {
                handle_connected_pong(&data[1..])?;
                Ok(true)
            }
            id::DISCONNECT_NOTIFICATION => {
                conn.close_immediately();
                Ok(true)
            }
            id::DETECT_LOST_CONNECTIONS => {
                let ping = ConnectedPing::new(timestamp());
                conn.send_message(&ping.write())?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn limits_enabled(&self) -> bool {
        true
    }

    fn close(&self, conn: &Conn) {
        // Remove the connection from the listener's connection map
        self.connections.write().remove(&conn.remote_addr());
    }
}

/// Handler for client-side (dialer) connections.
pub struct DialerConnectionHandler;

impl DialerConnectionHandler {
    /// Creates a new dialer connection handler.
    pub fn new() -> Self {
        Self
    }

    /// Handles a connection request accepted packet.
    pub fn handle_connection_request_accepted(&self, conn: &Conn, data: &[u8]) -> Result<()> {
        let pk = ConnectionRequestAccepted::read(data)?;

        // Send NewIncomingConnection before marking as connected
        let response = NewIncomingConnection::new(conn.remote_addr(), pk.pong_time, timestamp());
        conn.send_message(&response.write())?;
        conn.mark_connected();
        Ok(())
    }
}

impl Default for DialerConnectionHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectionHandler for DialerConnectionHandler {
    fn handle(&self, conn: &Conn, data: &[u8]) -> Result<bool> {
        if data.is_empty() {
            return Ok(false);
        }

        match data[0] {
            id::CONNECTION_REQUEST => Err(Error::UnexpectedPacket {
                packet_type: "CONNECTION_REQUEST",
            }),
            id::CONNECTION_REQUEST_ACCEPTED => {
                self.handle_connection_request_accepted(conn, &data[1..])?;
                Ok(true)
            }
            id::NEW_INCOMING_CONNECTION => Err(Error::UnexpectedPacket {
                packet_type: "NEW_INCOMING_CONNECTION",
            }),
            id::CONNECTED_PING => {
                handle_connected_ping(conn, &data[1..])?;
                Ok(true)
            }
            id::CONNECTED_PONG => {
                handle_connected_pong(&data[1..])?;
                Ok(true)
            }
            id::DISCONNECT_NOTIFICATION => {
                conn.close_immediately();
                Ok(true)
            }
            id::DETECT_LOST_CONNECTIONS => {
                let ping = ConnectedPing::new(timestamp());
                conn.send_message(&ping.write())?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn limits_enabled(&self) -> bool {
        false
    }

    fn close(&self, _conn: &Conn) {
        // Dialer handles its own cleanup
    }
}

/// Handles a connected ping packet.
fn handle_connected_ping(conn: &Conn, data: &[u8]) -> Result<()> {
    let pk = ConnectedPing::read(data)?;
    let response = ConnectedPong::new(pk.ping_time, timestamp());
    conn.send_message(&response.write())
}

/// Handles a connected pong packet.
fn handle_connected_pong(data: &[u8]) -> Result<()> {
    let pk = ConnectedPong::read(data)?;
    if pk.ping_time > timestamp() {
        return Err(Error::TimestampInFuture);
    }
    // We don't use the pong to measure RTT as it's too unreliable
    Ok(())
}
