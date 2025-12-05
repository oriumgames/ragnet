//! RakNet dialer implementation for establishing outgoing connections.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use rand::Rng as _;
use tokio::net::UdpSocket;
use tokio::time;

use crate::conn::Conn;
use crate::error::{Error, Result};
use crate::handler::DialerConnectionHandler;
use crate::message::{
    id, ConnectionRequest, IncompatibleProtocolVersion, OpenConnectionReply1, OpenConnectionReply2,
    OpenConnectionRequest1, OpenConnectionRequest2,
};
use crate::{timestamp, PROTOCOL_VERSION};

/// Configuration for dialing RakNet connections.
#[derive(Debug, Clone)]
pub struct Dialer {
    /// Error count before giving up.
    pub error_count: u32,

    /// Timeout for connection attempts.
    pub timeout: Duration,
}

impl Default for Dialer {
    fn default() -> Self {
        Self {
            error_count: 10,
            timeout: Duration::from_secs(10),
        }
    }
}

impl Dialer {
    /// Creates a new dialer with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the error count before giving up.
    pub fn with_error_count(mut self, count: u32) -> Self {
        self.error_count = count;
        self
    }

    /// Sets the connection timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Dials a RakNet server at the given address.
    pub async fn dial(&self, addr: &str) -> Result<Arc<Conn>> {
        let remote_addr: SocketAddr = addr
            .parse()
            .map_err(|e| Error::Other(format!("invalid address: {e}")))?;

        self.dial_addr(remote_addr).await
    }

    /// Dials a RakNet server at the given socket address.
    pub async fn dial_addr(&self, remote_addr: SocketAddr) -> Result<Arc<Conn>> {
        // Create a local UDP socket
        let local_addr = if remote_addr.is_ipv6() {
            "[::]:0"
        } else {
            "0.0.0.0:0"
        };

        let socket = UdpSocket::bind(local_addr).await.map_err(Error::Io)?;
        socket.connect(remote_addr).await.map_err(Error::Io)?;
        let socket = Arc::new(socket);

        let mut rng = rand::rng();
        let client_guid: i64 = rng.random();

        // Perform MTU discovery and connection handshake
        let (mtu, server_has_security, cookie) = time::timeout(
            self.timeout,
            self.discover_mtu(&socket, remote_addr, client_guid),
        )
        .await
        .map_err(|_| Error::Timeout)??;

        // Open connection
        let mtu = time::timeout(
            self.timeout,
            self.open_connection(
                &socket,
                remote_addr,
                client_guid,
                mtu,
                server_has_security,
                cookie,
            ),
        )
        .await
        .map_err(|_| Error::Timeout)??;

        // Create the connection
        let handler = Arc::new(DialerConnectionHandler::new());
        let conn = Conn::new(socket.clone(), remote_addr, mtu, handler);

        // Send connection request
        let request = ConnectionRequest::new(client_guid, timestamp());
        conn.send_message(&request.write())?;

        // Start receiving packets
        // The receive task will exit when the connection is closed (cancel_notify is triggered)
        let conn_clone = Arc::clone(&conn);
        let socket_clone = Arc::clone(&socket);
        let cancel = conn.get_cancel_notify();
        tokio::spawn(async move {
            let mut buf = vec![0u8; 1500];
            loop {
                tokio::select! {
                    result = socket_clone.recv(&mut buf) => {
                        match result {
                            Ok(n) if n > 0 => {
                                if let Err(e) = conn_clone.receive(&buf[..n]).await {
                                    tracing::debug!("receive error: {}", e);
                                    break;
                                }
                            }
                            Ok(_) => continue,
                            Err(e) => {
                                tracing::debug!("socket error: {}", e);
                                break;
                            }
                        }
                    }
                    _ = cancel.notified() => {
                        // Connection was closed, exit the receive loop
                        break;
                    }
                }
            }
        });

        // Wait for connection to be established
        let result = time::timeout(self.timeout, conn.wait_connected()).await;

        if result.is_err() {
            // Timeout occurred - close the connection to clean up the receive task
            conn.close_immediately();
            return Err(Error::Timeout);
        }

        Ok(conn)
    }

    const MTU_SIZES: [u16; 3] = [1492, 1200, 576];

    /// Discovers the MTU through the handshake process.
    async fn discover_mtu(
        &self,
        socket: &UdpSocket,
        _remote_addr: SocketAddr,
        _client_guid: i64,
    ) -> Result<(u16, bool, u32)> {
        let mut buf = vec![0u8; 1500];

        // Try each MTU size, 3 attempts per size
        for &mtu in &Self::MTU_SIZES {
            for _ in 0..3 {
                // Send OpenConnectionRequest1
                let request = OpenConnectionRequest1::new(PROTOCOL_VERSION, mtu);
                socket.send(&request.write()).await.map_err(Error::Io)?;

                // Wait for response with timeout
                let result = time::timeout(Duration::from_millis(500), socket.recv(&mut buf)).await;

                match result {
                    Ok(Ok(n)) if n > 0 => match buf[0] {
                        id::OPEN_CONNECTION_REPLY_1 => {
                            let reply = OpenConnectionReply1::read(&buf[1..n])?;
                            // Handle OVH DDoS protection quirk
                            if reply.server_guid == 0 || reply.mtu < 400 || reply.mtu > 1500 {
                                // Send a request2 followed by continuing with request1
                                // This deals with OVH's broken MTU response
                                continue;
                            }
                            return Ok((reply.mtu, reply.server_has_security, reply.cookie));
                        }
                        id::INCOMPATIBLE_PROTOCOL_VERSION => {
                            let pk = IncompatibleProtocolVersion::read(&buf[1..n])?;
                            return Err(Error::ProtocolMismatch {
                                client: PROTOCOL_VERSION,
                                server: pk.server_protocol,
                            });
                        }
                        _ => {
                            // Unknown packet, continue trying
                            continue;
                        }
                    },
                    Ok(Ok(_)) | Ok(Err(_)) | Err(_) => {
                        // Timeout or error, continue to next attempt
                        continue;
                    }
                }
            }
        }

        // All MTU sizes exhausted
        Err(Error::Timeout)
    }

    /// Opens a connection after MTU discovery.
    async fn open_connection(
        &self,
        socket: &UdpSocket,
        remote_addr: SocketAddr,
        client_guid: i64,
        mtu: u16,
        server_has_security: bool,
        cookie: u32,
    ) -> Result<u16> {
        let mut errors = 0;
        let mut buf = vec![0u8; 1500];

        loop {
            // Send OpenConnectionRequest2
            let request = if server_has_security {
                OpenConnectionRequest2::with_security(remote_addr, mtu, client_guid, cookie)
            } else {
                OpenConnectionRequest2::new(remote_addr, mtu, client_guid)
            };
            socket.send(&request.write()).await.map_err(Error::Io)?;

            // Wait for response with timeout
            let result = time::timeout(Duration::from_secs(2), socket.recv(&mut buf)).await;

            match result {
                Ok(Ok(n)) if n > 0 => {
                    match buf[0] {
                        id::OPEN_CONNECTION_REPLY_2 => {
                            let reply = OpenConnectionReply2::read(&buf[1..n])?;
                            return Ok(reply.mtu);
                        }
                        id::INCOMPATIBLE_PROTOCOL_VERSION => {
                            let pk = IncompatibleProtocolVersion::read(&buf[1..n])?;
                            return Err(Error::ProtocolMismatch {
                                client: PROTOCOL_VERSION,
                                server: pk.server_protocol,
                            });
                        }
                        id::OPEN_CONNECTION_REPLY_1 => {
                            // Got reply 1 again, server didn't receive our request 2
                            errors += 1;
                        }
                        _ => {
                            errors += 1;
                        }
                    }
                }
                Ok(Ok(_)) | Ok(Err(_)) | Err(_) => {
                    errors += 1;
                }
            }

            if errors >= self.error_count {
                return Err(Error::Timeout);
            }
        }
    }
}

/// Convenience function to dial a RakNet server.
pub async fn dial(addr: &str) -> Result<Arc<Conn>> {
    Dialer::new().dial(addr).await
}

/// Pings a RakNet server and returns the response data.
pub async fn ping(addr: &str) -> Result<Vec<u8>> {
    ping_timeout(addr, Duration::from_secs(5)).await
}

/// Pings a RakNet server with a custom timeout.
pub async fn ping_timeout(addr: &str, timeout: Duration) -> Result<Vec<u8>> {
    let remote_addr: SocketAddr = addr
        .parse()
        .map_err(|e| Error::Other(format!("invalid address: {e}")))?;

    let local_addr = if remote_addr.is_ipv6() {
        "[::]:0"
    } else {
        "0.0.0.0:0"
    };

    let socket = UdpSocket::bind(local_addr).await.map_err(Error::Io)?;

    let mut rng = rand::rng();
    let client_guid: i64 = rng.random();

    let ping = crate::message::UnconnectedPing::new(timestamp(), client_guid);
    socket
        .send_to(&ping.write(), remote_addr)
        .await
        .map_err(Error::Io)?;

    let mut buf = vec![0u8; 1500];

    let result = time::timeout(timeout, socket.recv_from(&mut buf)).await;

    match result {
        Ok(Ok((n, _))) if n > 0 && buf[0] == id::UNCONNECTED_PONG => {
            let pong = crate::message::UnconnectedPong::read(&buf[1..n])?;
            Ok(pong.data)
        }
        Ok(Ok(_)) => Err(Error::Timeout),
        Ok(Err(e)) => Err(Error::Io(e)),
        Err(_) => Err(Error::Timeout),
    }
}
