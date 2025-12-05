//! RakNet connection implementation.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use parking_lot::RwLock;
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, Mutex, Notify};
use tokio::time;

use crate::acknowledge::Acknowledgement;
use crate::binary::{load_uint24, write_uint24, Uint24};
use crate::datagram_window::DatagramWindow;
use crate::error::{Error, Result};
use crate::handler::ConnectionHandler;
use crate::message::id;
use crate::packet::{
    split, Packet, BIT_FLAG_ACK, BIT_FLAG_DATAGRAM, BIT_FLAG_NACK, BIT_FLAG_NEEDS_B_AND_AS,
    RELIABILITY_RELIABLE_ORDERED,
};
use crate::packet_queue::PacketQueue;
use crate::resend_map::ResendMap;
use crate::{timestamp, MAX_MTU_SIZE, MAX_WINDOW_SIZE, MIN_MTU_SIZE};

/// Maximum number of split fragments.
const MAX_SPLIT_COUNT: u32 = 512;

/// Maximum number of concurrent split packets.
const MAX_CONCURRENT_SPLITS: usize = 16;

/// Represents a RakNet connection.
pub struct Conn {
    /// The underlying UDP socket.
    socket: Arc<UdpSocket>,

    /// Remote address of the peer.
    remote_addr: SocketAddr,

    /// Connection handler for protocol messages.
    handler: Arc<dyn ConnectionHandler>,

    /// MTU size for this connection.
    mtu: u16,

    /// Round-trip time in nanoseconds.
    rtt: AtomicI64,

    /// Unix timestamp when closing started, 0 if not closing.
    closing: AtomicI64,

    /// Whether the connection has been established.
    connected: AtomicBool,

    /// Notifier for when connection is established.
    connected_notify: Notify,

    /// Whether close has been called.
    closed: AtomicBool,

    /// Cancellation notifier.
    cancel_notify: Arc<Notify>,

    /// Mutex-protected connection state (async-safe).
    state: Arc<Mutex<ConnState>>,

    /// Channel for received packets.
    packet_tx: mpsc::Sender<Bytes>,
    packet_rx: Mutex<mpsc::Receiver<Bytes>>,

    /// Channel for outgoing datagrams (avoids spawning tasks per send).
    send_tx: mpsc::Sender<Bytes>,

    /// Last activity timestamp.
    last_activity: RwLock<Instant>,

    /// Buffer for ACK packets.
    ack_buf: Mutex<Vec<u8>>,

    /// Buffer for NACK packets.
    nack_buf: Mutex<Vec<u8>>,
}

/// Mutable connection state protected by a mutex.
struct ConnState {
    /// Buffer for building datagrams.
    buf: Vec<u8>,

    /// Reusable packet for reading.
    pk: Packet,

    /// Sequence number counter.
    seq: Uint24,

    /// Order index counter.
    order_index: Uint24,

    /// Message index counter.
    message_index: Uint24,

    /// Split ID counter.
    split_id: u32,

    /// Map of split packets being reassembled.
    splits: HashMap<u16, Vec<Vec<u8>>>,

    /// Datagram window for tracking received datagrams.
    win: DatagramWindow,

    /// Pending ACK sequence numbers.
    ack_slice: Vec<Uint24>,

    /// Packet queue for ordered delivery.
    packet_queue: PacketQueue,

    /// Retransmission tracking.
    retransmission: ResendMap,
}

impl Conn {
    /// Creates a new connection.
    pub(crate) fn new(
        socket: Arc<UdpSocket>,
        remote_addr: SocketAddr,
        mtu: u16,
        handler: Arc<dyn ConnectionHandler>,
    ) -> Arc<Self> {
        let mtu = mtu.clamp(MIN_MTU_SIZE, MAX_MTU_SIZE);
        let (packet_tx, packet_rx) = mpsc::channel(4096);
        let (send_tx, send_rx) = mpsc::channel::<Bytes>(256);

        let conn = Arc::new(Self {
            socket,
            remote_addr,
            handler,
            mtu,
            rtt: AtomicI64::new(0),
            closing: AtomicI64::new(0),
            connected: AtomicBool::new(false),
            connected_notify: Notify::new(),
            closed: AtomicBool::new(false),
            cancel_notify: Arc::new(Notify::new()),
            state: Arc::new(Mutex::new(ConnState {
                buf: Vec::with_capacity((mtu - 28) as usize),
                pk: Packet::new(),
                seq: Uint24::new(0),
                order_index: Uint24::new(0),
                message_index: Uint24::new(0),
                split_id: 0,
                splits: HashMap::new(),
                win: DatagramWindow::new(),
                ack_slice: Vec::new(),
                packet_queue: PacketQueue::new(),
                retransmission: ResendMap::new(),
            })),
            packet_tx,
            packet_rx: Mutex::new(packet_rx),
            last_activity: RwLock::new(Instant::now()),
            ack_buf: Mutex::new(Vec::with_capacity(128)),
            nack_buf: Mutex::new(Vec::with_capacity(64)),
            send_tx,
        });

        // Start the ticker task
        let conn_clone = Arc::clone(&conn);
        tokio::spawn(async move {
            conn_clone.start_ticking().await;
        });

        // Start the sender task
        let socket = Arc::clone(&conn.socket);
        let remote_addr = conn.remote_addr;
        let cancel = Arc::clone(&conn.cancel_notify);
        tokio::spawn(async move {
            Self::sender_task(socket, remote_addr, send_rx, cancel).await;
        });

        conn
    }

    /// Returns the effective MTU (minus IP/UDP headers).
    pub fn effective_mtu(&self) -> u16 {
        self.mtu - 28
    }

    /// Returns the remote address.
    pub fn remote_addr(&self) -> SocketAddr {
        self.remote_addr
    }

    /// Returns the local address.
    pub fn local_addr(&self) -> Result<SocketAddr> {
        self.socket.local_addr().map_err(Error::Io)
    }

    /// Returns the current latency (half of RTT).
    pub fn latency(&self) -> Duration {
        Duration::from_nanos((self.rtt.load(Ordering::Relaxed) / 2) as u64)
    }

    /// Returns a clone of the cancel notify for use in receive loops.
    pub(crate) fn get_cancel_notify(&self) -> Arc<Notify> {
        Arc::clone(&self.cancel_notify)
    }

    /// Marks the connection as established.
    pub(crate) fn mark_connected(&self) {
        if !self.connected.swap(true, Ordering::SeqCst) {
            self.connected_notify.notify_waiters();
        }
    }

    /// Waits for the connection to be established.
    pub async fn wait_connected(&self) {
        if self.connected.load(Ordering::SeqCst) {
            return;
        }
        self.connected_notify.notified().await;
    }

    /// Returns whether the connection is established.
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    /// Writes data to the connection.
    pub async fn write(&self, data: &[u8]) -> Result<usize> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(Error::ConnectionClosed);
        }

        let mut state = self.state.lock().await;
        self.write_locked(&mut state, data).await
    }

    /// Internal write with lock already held.
    async fn write_locked(&self, state: &mut ConnState, data: &[u8]) -> Result<usize> {
        let fragments = split(data, self.effective_mtu());
        let order_index = state.order_index.inc();

        let split_id = state.split_id as u16;
        if fragments.len() > 1 {
            state.split_id = state.split_id.wrapping_add(1);
        }

        let mut n = 0;
        for (split_index, content) in fragments.iter().enumerate() {
            let mut pk = Packet::new();
            pk.content = content.to_vec();
            pk.order_index = order_index;
            pk.message_index = state.message_index.inc();

            if fragments.len() > 1 {
                pk.split = true;
                pk.split_count = fragments.len() as u32;
                pk.split_index = split_index as u32;
                pk.split_id = split_id;
            }

            self.send_datagram(state, pk).await?;
            n += content.len();
        }

        Ok(n)
    }

    /// Reads a packet from the connection.
    pub async fn read(&self, buf: &mut [u8]) -> Result<usize> {
        let mut rx = self.packet_rx.lock().await;

        tokio::select! {
            result = rx.recv() => {
                match result {
                    Some(data) => {
                        let len = data.len();
                        if buf.len() < len {
                            return Err(Error::BufferTooSmall);
                        }
                        buf[..len].copy_from_slice(&data);
                        Ok(len)
                    }
                    None => Err(Error::ConnectionClosed),
                }
            }
            _ = self.cancel_notify.notified() => {
                Err(Error::ConnectionClosed)
            }
        }
    }

    /// Reads a packet and returns it as a Vec.
    pub async fn read_packet(&self) -> Result<Bytes> {
        let mut rx = self.packet_rx.lock().await;

        tokio::select! {
            result = rx.recv() => {
                result.ok_or(Error::ConnectionClosed)
            }
            _ = self.cancel_notify.notified() => {
                Err(Error::ConnectionClosed)
            }
        }
    }

    /// Dedicated sender task that processes outgoing datagrams.
    async fn sender_task(
        socket: Arc<UdpSocket>,
        remote_addr: SocketAddr,
        mut rx: mpsc::Receiver<Bytes>,
        cancel: Arc<Notify>,
    ) {
        loop {
            tokio::select! {
                Some(data) = rx.recv() => {
                    if let Err(e) = socket.send_to(&data, remote_addr).await {
                        tracing::debug!("failed to send to {}: {}", remote_addr, e);
                    }
                }
                _ = cancel.notified() => {
                    // Drain remaining messages
                    while let Ok(data) = rx.try_recv() {
                        let _ = socket.send_to(&data, remote_addr).await;
                    }
                    return;
                }
            }
        }
    }

    /// Closes the connection gracefully.
    pub async fn close(&self) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        self.closing
            .compare_exchange(0, now, Ordering::SeqCst, Ordering::SeqCst)
            .ok();
        Ok(())
    }

    /// Closes the connection immediately.
    pub(crate) fn close_immediately(&self) {
        if self.closed.swap(true, Ordering::SeqCst) {
            return;
        }

        // Send disconnect notification
        let _ = self.send_message(&[id::DISCONNECT_NOTIFICATION]);

        self.handler.close(self);
        self.cancel_notify.notify_waiters();
    }

    /// Sends a message (internal protocol message).
    /// Uses the dedicated send channel to avoid spawning tasks.
    pub(crate) fn send_message(&self, data: &[u8]) -> Result<()> {
        let data = data.to_vec();
        let state = self.state.clone();
        let send_tx = self.send_tx.clone();

        tokio::spawn(async move {
            let mut state = state.lock().await;

            // For internal messages, we need to wrap them in a datagram
            let mut pk = Packet::new();
            pk.content = data;
            pk.order_index = state.order_index.inc();
            pk.message_index = state.message_index.inc();

            // Build datagram into a reusable buffer
            let mut buf = Vec::with_capacity(128);
            buf.push(BIT_FLAG_DATAGRAM | BIT_FLAG_NEEDS_B_AND_AS);
            let seq = state.seq.inc();
            write_uint24(&mut buf, seq);
            pk.write(&mut buf);

            state.retransmission.add(seq, pk);
            drop(state);

            // Send via channel (non-blocking)
            let _ = send_tx.try_send(Bytes::from(buf));
        });

        Ok(())
    }

    /// Sends a datagram containing a packet.
    async fn send_datagram(&self, state: &mut ConnState, pk: Packet) -> Result<()> {
        state.buf.clear();
        state.buf.push(BIT_FLAG_DATAGRAM | BIT_FLAG_NEEDS_B_AND_AS);
        let seq = state.seq.inc();
        write_uint24(&mut state.buf, seq);
        pk.write(&mut state.buf);

        // Clone into Bytes for zero-copy sending
        let data = Bytes::copy_from_slice(&state.buf);
        state.retransmission.add(seq, pk);

        // Use channel for consistent send path
        self.send_tx
            .send(data)
            .await
            .map_err(|_| Error::ConnectionClosed)?;
        Ok(())
    }

    /// Receives and processes a packet from the network.
    pub(crate) async fn receive(&self, data: &[u8]) -> Result<()> {
        *self.last_activity.write() = Instant::now();

        if data.is_empty() {
            return Ok(());
        }

        match data[0] {
            b if b & BIT_FLAG_ACK != 0 => self.handle_ack(&data[1..]).await,
            b if b & BIT_FLAG_NACK != 0 => self.handle_nack(&data[1..]).await,
            b if b & BIT_FLAG_DATAGRAM != 0 => self.receive_datagram(&data[1..]).await,
            _ => Ok(()),
        }
    }

    /// Handles a received datagram.
    async fn receive_datagram(&self, data: &[u8]) -> Result<()> {
        if data.len() < 3 {
            return Err(Error::UnexpectedEof);
        }

        let seq = load_uint24(data);

        // Process under lock, collect any NACKs needed
        let (_should_handle, missing, window_error) = {
            let mut state = self.state.lock().await;

            if !state.win.add(seq) {
                // Already received this datagram
                return Ok(());
            }

            // Add to pending ACKs
            state.ack_slice.push(seq);

            let missing = if state.win.shift() == 0 {
                // Window couldn't be shifted, we're missing packets
                let rtt = Duration::from_nanos(self.rtt.load(Ordering::Relaxed) as u64);
                state.win.missing(rtt + rtt / 2)
            } else {
                Vec::new()
            };

            let window_error =
                if state.win.size() > MAX_WINDOW_SIZE && self.handler.limits_enabled() {
                    Some((state.win.lowest.value(), state.win.highest.value()))
                } else {
                    None
                };

            (true, missing, window_error)
        };

        // Send NACK outside of lock
        if !missing.is_empty() {
            self.send_nack(&missing).await?;
        }

        if let Some((lowest, highest)) = window_error {
            return Err(Error::WindowSizeTooBig { lowest, highest });
        }

        // Handle datagram contents
        self.handle_datagram(&data[3..]).await
    }

    /// Handles datagram contents.
    async fn handle_datagram(&self, mut data: &[u8]) -> Result<()> {
        while !data.is_empty() {
            let (pk, n) = {
                let mut state = self.state.lock().await;
                let n = state.pk.read(data)?;
                let pk = std::mem::take(&mut state.pk);
                state.pk = Packet::new();
                (pk, n)
            };
            data = &data[n..];

            if pk.split {
                self.receive_split_packet(pk).await?;
            } else {
                self.receive_packet(pk).await?;
            }
        }
        Ok(())
    }

    /// Receives a non-split packet.
    async fn receive_packet(&self, pk: Packet) -> Result<()> {
        if pk.reliability != RELIABILITY_RELIABLE_ORDERED {
            // Handle immediately for non-ordered packets
            return self.handle_packet(pk.content).await;
        }

        let packets_to_handle = {
            let mut state = self.state.lock().await;

            if !state.packet_queue.put(pk.order_index, pk.content) {
                // Duplicate packet
                return Ok(());
            }

            if state.packet_queue.window_size() > MAX_WINDOW_SIZE && self.handler.limits_enabled() {
                return Err(Error::WindowSizeTooBig {
                    lowest: state.packet_queue.lowest.value(),
                    highest: state.packet_queue.highest.value(),
                });
            }

            state.packet_queue.fetch()
        };

        for content in packets_to_handle {
            self.handle_packet(content).await?;
        }

        Ok(())
    }

    /// Receives a split packet fragment.
    async fn receive_split_packet(&self, pk: Packet) -> Result<()> {
        let combined_pk = {
            let mut state = self.state.lock().await;

            if pk.split_count > MAX_SPLIT_COUNT && self.handler.limits_enabled() {
                return Err(Error::SplitPacket(format!(
                    "split count {} exceeds maximum {}",
                    pk.split_count, MAX_SPLIT_COUNT
                )));
            }

            if state.splits.len() > MAX_CONCURRENT_SPLITS && self.handler.limits_enabled() {
                return Err(Error::SplitPacket(format!(
                    "maximum concurrent splits {MAX_CONCURRENT_SPLITS} reached"
                )));
            }

            let fragments = state
                .splits
                .entry(pk.split_id)
                .or_insert_with(|| vec![Vec::new(); pk.split_count as usize]);

            if pk.split_index >= fragments.len() as u32 {
                return Err(Error::SplitPacket(format!(
                    "split index {} is out of range (0 - {})",
                    pk.split_index,
                    fragments.len() - 1
                )));
            }

            fragments[pk.split_index as usize] = pk.content.clone();

            // Check if all fragments received
            if fragments.iter().any(|f| f.is_empty()) {
                return Ok(());
            }

            // Combine fragments
            let combined: Vec<u8> = fragments.iter().flatten().copied().collect();
            state.splits.remove(&pk.split_id);

            let mut combined_pk = Packet::new();
            combined_pk.content = combined;
            combined_pk.order_index = pk.order_index;
            combined_pk.reliability = pk.reliability;
            combined_pk
        };

        self.receive_packet(combined_pk).await
    }

    /// Handles a complete packet.
    async fn handle_packet(&self, content: Vec<u8>) -> Result<()> {
        if content.is_empty() {
            return Err(Error::ZeroPacket);
        }

        if self.closing.load(Ordering::Relaxed) != 0 {
            return Ok(());
        }

        // Check if handler wants to process this packet
        let handled = self.handler.handle(self, &content)?;

        if !handled {
            // Send to user - convert to Bytes for zero-copy
            let _ = self.packet_tx.send(Bytes::from(content)).await;
        }

        Ok(())
    }

    /// Handles an ACK packet.
    async fn handle_ack(&self, data: &[u8]) -> Result<()> {
        let mut ack = Acknowledgement::new();
        ack.read(data)?;

        let mut state = self.state.lock().await;
        for seq in ack.packets {
            state.retransmission.acknowledge(seq);
        }

        Ok(())
    }

    /// Handles a NACK packet.
    async fn handle_nack(&self, data: &[u8]) -> Result<()> {
        let mut nack = Acknowledgement::new();
        nack.read(data)?;

        let mut state = self.state.lock().await;

        for seq in nack.packets {
            if let Some(pk) = state.retransmission.retransmit(seq) {
                let new_seq = state.seq.inc();

                let mut buf = Vec::with_capacity(128);
                buf.push(BIT_FLAG_DATAGRAM | BIT_FLAG_NEEDS_B_AND_AS);
                write_uint24(&mut buf, new_seq);
                pk.write(&mut buf);

                // Add the packet to retransmission queue with the NEW sequence number
                state.retransmission.add(new_seq, pk);

                // Send via channel instead of spawning a task
                let _ = self.send_tx.try_send(Bytes::from(buf));
            }
        }

        Ok(())
    }

    /// Sends an ACK for the given sequence numbers.
    async fn send_ack(&self, packets: &[Uint24]) -> Result<()> {
        let mut buf = self.ack_buf.lock().await;
        self.send_acknowledgement(packets, BIT_FLAG_ACK, &mut buf)
            .await
    }

    /// Sends a NACK for the given sequence numbers.
    async fn send_nack(&self, packets: &[Uint24]) -> Result<()> {
        let mut buf = self.nack_buf.lock().await;
        self.send_acknowledgement(packets, BIT_FLAG_NACK, &mut buf)
            .await
    }

    /// Sends an acknowledgement packet with the given packets and bitflag.
    /// Handles splitting into multiple packets if needed.
    async fn send_acknowledgement(
        &self,
        packets: &[Uint24],
        bitflag: u8,
        buf: &mut Vec<u8>,
    ) -> Result<()> {
        let mut ack = Acknowledgement::with_packets(packets.to_vec());
        let mtu = self.effective_mtu();

        while !ack.packets.is_empty() {
            buf.clear();
            buf.push(bitflag | BIT_FLAG_DATAGRAM);

            let n = ack.write_to_buf(buf, mtu);

            // Remove the packets we managed to write
            ack.packets = ack.packets.split_off(n);

            // Send via channel
            let _ = self.send_tx.send(Bytes::copy_from_slice(buf)).await;
        }

        buf.clear();
        Ok(())
    }

    /// Flushes pending ACKs.
    async fn flush_acks(&self) {
        let packets: Vec<Uint24> = {
            let mut state = self.state.lock().await;
            if state.ack_slice.is_empty() {
                return;
            }
            std::mem::take(&mut state.ack_slice)
        };

        let _ = self.send_ack(&packets).await;
    }

    /// Checks for packets that need retransmission.
    async fn check_resend(&self, now: Instant) {
        let mut state = self.state.lock().await;

        let rtt = state.retransmission.rtt(now);
        let delay = rtt + rtt / 2;
        self.rtt.store(rtt.as_nanos() as i64, Ordering::Relaxed);

        let resend: Vec<Uint24> = state
            .retransmission
            .unacknowledged
            .iter()
            .filter(|(_, record)| now.duration_since(record.timestamp) > delay)
            .map(|(&seq, _)| Uint24::new(seq))
            .collect();

        for seq in resend {
            if let Some(pk) = state.retransmission.retransmit(seq) {
                let new_seq = state.seq.inc();

                let mut buf = Vec::with_capacity(128);
                buf.push(BIT_FLAG_DATAGRAM | BIT_FLAG_NEEDS_B_AND_AS);
                write_uint24(&mut buf, new_seq);
                pk.write(&mut buf);

                // Add the packet to retransmission queue with the NEW sequence number
                state.retransmission.add(new_seq, pk);

                // Send via channel instead of spawning a task
                let _ = self.send_tx.try_send(Bytes::from(buf));
            }
        }
    }

    /// Main tick loop for the connection.
    async fn start_ticking(self: Arc<Self>) {
        let mut interval = time::interval(Duration::from_millis(100));
        let mut tick_count: u64 = 0;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    tick_count += 1;

                    self.flush_acks().await;

                    if tick_count % 3 == 0 {
                        self.check_resend(Instant::now()).await;
                    }

                    let closing_time = self.closing.load(Ordering::Relaxed);
                    if closing_time != 0 {
                        let acks_left = {
                            let state = self.state.lock().await;
                            state.retransmission.unacknowledged.len()
                        };

                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs() as i64;
                        let since = Duration::from_secs((now - closing_time) as u64);

                        if (acks_left == 0 && since > Duration::from_secs(1))
                            || since > Duration::from_secs(5)
                        {
                            self.close_immediately();
                            return;
                        }
                        continue;
                    }

                    if tick_count % 5 == 0 {
                        // Send ping
                        let ping = crate::message::ConnectedPing::new(timestamp());
                        let _ = self.send_message(&ping.write());

                        // Check for timeout
                        let last_activity = *self.last_activity.read();
                        let rtt = Duration::from_nanos(self.rtt.load(Ordering::Relaxed) as u64);
                        if Instant::now().duration_since(last_activity) > Duration::from_secs(5) + rtt * 2 {
                            let _ = self.close().await;
                        }
                    }
                }
                _ = self.cancel_notify.notified() => {
                    return;
                }
            }
        }
    }
}
