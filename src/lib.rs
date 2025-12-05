//! # RakNet
//!
//! A Rust library implementing a basic version of the RakNet protocol,
//! which is used for Minecraft (Bedrock Edition).
//!
//! This library implements Unreliable, Reliable and ReliableOrdered packets
//! and sends user packets as ReliableOrdered.
//!
//! ## Example Server
//!
//! ```no_run
//! use ragnet::Listener;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let listener = Listener::bind("0.0.0.0:19132").await?;
//!
//!     loop {
//!         let conn = listener.accept().await?;
//!         tokio::spawn(async move {
//!             let mut buf = vec![0u8; 4 * 1024 * 1024];
//!             if let Ok(_n) = conn.read(&mut buf).await {
//!                 let _ = conn.write(&[1, 2, 3]).await;
//!             }
//!             let _ = conn.close().await;
//!         });
//!     }
//! }
//! ```
//!
//! ## Example Client
//!
//! ```no_run
//! use ragnet::Dialer;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let conn = Dialer::new().dial("127.0.0.1:19132").await?;
//!
//!     conn.write(&[1, 2, 3]).await?;
//!
//!     let mut buf = vec![0u8; 4 * 1024 * 1024];
//!     let _n = conn.read(&mut buf).await?;
//!
//!     conn.close().await?;
//!     Ok(())
//! }
//! ```

pub mod acknowledge;
pub mod binary;
pub mod conn;
pub mod datagram_window;
pub mod dial;
pub mod error;
pub mod handler;
pub mod listener;
pub mod message;
pub mod packet;
pub mod packet_queue;
pub mod resend_map;

// Re-exports
pub use conn::Conn;
pub use dial::Dialer;
pub use error::{Error, Result};
pub use listener::{ListenConfig, Listener};

/// The current RakNet protocol version. This is Minecraft specific.
pub const PROTOCOL_VERSION: u8 = 11;

/// Minimum MTU size allowed.
pub const MIN_MTU_SIZE: u16 = 400;

/// Maximum MTU size allowed.
pub const MAX_MTU_SIZE: u16 = 1492;

/// Maximum window size for datagrams.
pub const MAX_WINDOW_SIZE: u32 = 2048;

/// Returns a timestamp in milliseconds.
pub fn timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}
