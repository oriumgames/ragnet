//! RakNet listener implementation for accepting incoming connections.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::{Mutex, RwLock};
use rand::Rng as _;
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, Mutex as TokioMutex};

use crate::conn::Conn;
use crate::error::{Error, Result};
use crate::handler::ListenerConnectionHandler;
use crate::message::{
    id, IncompatibleProtocolVersion, OpenConnectionReply1, OpenConnectionReply2,
    OpenConnectionRequest1, OpenConnectionRequest2, UnconnectedPing, UnconnectedPong,
};
use crate::packet::BIT_FLAG_DATAGRAM;
use crate::{MAX_MTU_SIZE, PROTOCOL_VERSION};

/// Configuration for a RakNet listener.
#[derive(Debug, Clone)]
pub struct ListenConfig {
    /// Whether to disable cookie-based security.
    pub disable_cookies: bool,

    /// Duration to block IP addresses after errors.
    pub block_duration: Duration,
}

impl Default for ListenConfig {
    fn default() -> Self {
        Self {
            disable_cookies: false,
            block_duration: Duration::from_secs(10),
        }
    }
}

/// A RakNet connection listener.
pub struct Listener {
    /// Configuration.
    config: ListenConfig,

    /// The underlying UDP socket.
    socket: Arc<UdpSocket>,

    /// Server GUID.
    id: i64,

    /// Cookie salt for security.
    cookie_salt: u32,

    /// Whether the listener is closed.
    closed: AtomicBool,

    /// Channel for incoming connections.
    incoming_tx: mpsc::Sender<Arc<Conn>>,
    incoming_rx: TokioMutex<mpsc::Receiver<Arc<Conn>>>,

    /// Active connections by address.
    connections: Arc<RwLock<HashMap<SocketAddr, Arc<Conn>>>>,

    /// Blocked IP addresses.
    blocks: Mutex<HashMap<[u8; 16], Instant>>,

    /// Block count for fast-path checking.
    block_count: AtomicU32,

    /// Pong data to send in response to pings.
    pong_data: RwLock<Vec<u8>>,
}

impl Listener {
    /// Creates a new listener bound to the given address.
    pub async fn bind(addr: &str) -> Result<Arc<Self>> {
        ListenConfig::default().listen(addr).await
    }

    /// Accepts an incoming connection.
    pub async fn accept(&self) -> Result<Arc<Conn>> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(Error::ListenerClosed);
        }

        let mut rx = self.incoming_rx.lock().await;
        rx.recv().await.ok_or(Error::ListenerClosed)
    }

    /// Returns the local address of the listener.
    pub fn local_addr(&self) -> Result<SocketAddr> {
        self.socket.local_addr().map_err(Error::Io)
    }

    /// Returns the server ID.
    pub fn id(&self) -> i64 {
        self.id
    }

    /// Sets the pong data to send in response to pings.
    pub fn set_pong_data(&self, data: Vec<u8>) {
        if data.len() > i16::MAX as usize {
            panic!("pong data must be no longer than {} bytes", i16::MAX);
        }
        *self.pong_data.write() = data;
    }

    /// Closes the listener.
    pub async fn close(&self) -> Result<()> {
        if self.closed.swap(true, Ordering::SeqCst) {
            return Ok(());
        }

        // Close all connections
        let connections: Vec<_> = self.connections.read().values().cloned().collect();
        for conn in connections {
            conn.close_immediately();
        }

        Ok(())
    }

    /// Main listen loop.
    async fn listen(self: Arc<Self>) {
        let mut buf = vec![0u8; 1500];

        loop {
            if self.closed.load(Ordering::SeqCst) {
                return;
            }

            let (n, addr) = match self.socket.recv_from(&mut buf).await {
                Ok(result) => result,
                Err(e) => {
                    if self.closed.load(Ordering::SeqCst) {
                        return;
                    }
                    tracing::error!("read from: {}", e);
                    continue;
                }
            };

            if n == 0 || self.is_blocked(&addr) {
                continue;
            }

            if let Err(e) = self.handle(&buf[..n], addr).await {
                tracing::error!("handle packet from {}: {}", addr, e);
                self.block(&addr);
            }
        }
    }

    /// Handles an incoming packet.
    async fn handle(self: &Arc<Self>, data: &[u8], addr: SocketAddr) -> Result<()> {
        // Check if this is from an existing connection
        let conn = self.connections.read().get(&addr).cloned();
        if let Some(conn) = conn {
            return conn.receive(data).await;
        }

        // Handle unconnected packets
        self.handle_unconnected(data, addr).await
    }

    /// Handles an unconnected packet.
    async fn handle_unconnected(self: &Arc<Self>, data: &[u8], addr: SocketAddr) -> Result<()> {
        if data.is_empty() {
            return Ok(());
        }

        match data[0] {
            id::UNCONNECTED_PING | id::UNCONNECTED_PING_OPEN_CONNECTIONS => {
                self.handle_unconnected_ping(&data[1..], addr).await
            }
            id::OPEN_CONNECTION_REQUEST_1 => {
                self.handle_open_connection_request_1(&data[1..], addr)
                    .await
            }
            id::OPEN_CONNECTION_REQUEST_2 => {
                self.handle_open_connection_request_2(&data[1..], addr)
                    .await
            }
            b if b & BIT_FLAG_DATAGRAM != 0 => {
                // Unexpected datagram from unknown connection
                tracing::debug!("unexpected datagram from {}", addr);
                Ok(())
            }
            _ => Err(Error::UnknownPacket {
                id: data[0],
                len: data.len(),
            }),
        }
    }

    /// Handles an unconnected ping.
    async fn handle_unconnected_ping(&self, data: &[u8], addr: SocketAddr) -> Result<()> {
        let pk = UnconnectedPing::read(data)?;
        let pong_data = self.pong_data.read().clone();
        let response = UnconnectedPong::new(pk.ping_time, self.id, pong_data);
        self.socket
            .send_to(&response.write(), addr)
            .await
            .map_err(Error::Io)?;
        Ok(())
    }

    /// Handles open connection request 1.
    async fn handle_open_connection_request_1(&self, data: &[u8], addr: SocketAddr) -> Result<()> {
        let pk = OpenConnectionRequest1::read(data)?;
        let mtu = pk.mtu.min(MAX_MTU_SIZE);

        if pk.client_protocol != PROTOCOL_VERSION {
            let response = IncompatibleProtocolVersion::new(PROTOCOL_VERSION, self.id);
            self.socket
                .send_to(&response.write(), addr)
                .await
                .map_err(Error::Io)?;
            return Err(Error::ProtocolMismatch {
                client: pk.client_protocol,
                server: PROTOCOL_VERSION,
            });
        }

        let cookie = self.cookie(&addr);
        let response = if self.config.disable_cookies {
            OpenConnectionReply1::new(self.id, mtu)
        } else {
            OpenConnectionReply1::with_security(self.id, cookie, mtu)
        };

        self.socket
            .send_to(&response.write(), addr)
            .await
            .map_err(Error::Io)?;
        Ok(())
    }

    /// Handles open connection request 2.
    async fn handle_open_connection_request_2(
        self: &Arc<Self>,
        data: &[u8],
        addr: SocketAddr,
    ) -> Result<()> {
        let mut pk = OpenConnectionRequest2 {
            server_has_security: !self.config.disable_cookies,
            ..Default::default()
        };
        pk.read(data)?;

        let expected_cookie = self.cookie(&addr);
        if pk.cookie != expected_cookie {
            return Err(Error::InvalidCookie {
                got: pk.cookie,
                expected: expected_cookie,
            });
        }

        let mtu = pk.mtu.min(MAX_MTU_SIZE);

        // Send reply
        let response = OpenConnectionReply2::new(self.id, addr, mtu);
        self.socket
            .send_to(&response.write(), addr)
            .await
            .map_err(Error::Io)?;

        // Create connection
        let handler = Arc::new(ListenerConnectionHandler::new(
            self.cookie_salt,
            self.config.disable_cookies,
            Arc::clone(&self.connections),
        ));

        let conn = Conn::new(Arc::clone(&self.socket), addr, mtu, handler);

        self.connections.write().insert(addr, Arc::clone(&conn));

        // Wait for connection to be established with timeout
        let listener = Arc::clone(self);
        let conn_clone = Arc::clone(&conn);
        tokio::spawn(async move {
            tokio::select! {
                _ = conn_clone.wait_connected() => {
                    // Connection established, add to incoming
                    let _ = listener.incoming_tx.send(conn_clone).await;
                }
                _ = tokio::time::sleep(Duration::from_secs(10)) => {
                    // Timeout
                    conn_clone.close_immediately();
                    listener.connections.write().remove(&addr);
                }
            }
        });

        Ok(())
    }

    /// Calculates a cookie for the given address.
    fn cookie(&self, addr: &SocketAddr) -> u32 {
        if self.config.disable_cookies {
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

    /// Blocks an IP address.
    fn block(&self, addr: &SocketAddr) {
        if self.config.block_duration.is_zero() {
            return;
        }

        let ip = match addr.ip() {
            std::net::IpAddr::V4(ip) => ip.to_ipv6_mapped().octets(),
            std::net::IpAddr::V6(ip) => ip.octets(),
        };

        self.block_count.fetch_add(1, Ordering::Relaxed);
        self.blocks.lock().insert(ip, Instant::now());
    }

    /// Checks if an IP address is blocked.
    fn is_blocked(&self, addr: &SocketAddr) -> bool {
        if self.config.block_duration.is_zero() || self.block_count.load(Ordering::Relaxed) == 0 {
            return false;
        }

        let ip = match addr.ip() {
            std::net::IpAddr::V4(ip) => ip.to_ipv6_mapped().octets(),
            std::net::IpAddr::V6(ip) => ip.octets(),
        };

        self.blocks.lock().contains_key(&ip)
    }

    /// Garbage collects expired blocks.
    async fn gc_blocks(self: Arc<Self>) {
        let mut interval = tokio::time::interval(Duration::from_secs(1));

        loop {
            interval.tick().await;

            if self.closed.load(Ordering::SeqCst) {
                return;
            }

            if self.block_count.load(Ordering::Relaxed) == 0 {
                continue;
            }

            let now = Instant::now();
            let mut blocks = self.blocks.lock();
            blocks.retain(|_, t| now.duration_since(*t) <= self.config.block_duration);
            self.block_count
                .store(blocks.len() as u32, Ordering::Relaxed);
        }
    }
}

impl ListenConfig {
    /// Creates a listener with this configuration.
    pub async fn listen(&self, addr: &str) -> Result<Arc<Listener>> {
        let socket = UdpSocket::bind(addr).await.map_err(Error::Io)?;
        let socket = Arc::new(socket);

        let (incoming_tx, incoming_rx) = mpsc::channel(64);

        let mut rng = rand::rng();

        let listener = Arc::new(Listener {
            config: self.clone(),
            socket,
            id: rng.random(),
            cookie_salt: rng.random(),
            closed: AtomicBool::new(false),
            incoming_tx,
            incoming_rx: TokioMutex::new(incoming_rx),
            connections: Arc::new(RwLock::new(HashMap::new())),
            blocks: Mutex::new(HashMap::new()),
            block_count: AtomicU32::new(0),
            pong_data: RwLock::new(Vec::new()),
        });

        // Start listen loop
        let listener_clone = Arc::clone(&listener);
        tokio::spawn(async move {
            listener_clone.listen().await;
        });

        // Start block GC
        let listener_clone = Arc::clone(&listener);
        tokio::spawn(async move {
            listener_clone.gc_blocks().await;
        });

        Ok(listener)
    }
}

/// Convenience function to create a listener.
pub async fn listen(addr: &str) -> Result<Arc<Listener>> {
    Listener::bind(addr).await
}

/// Alias for listen.
pub use listen as Listen;
