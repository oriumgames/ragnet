# ragnet

A Rust implementation of the RakNet protocol, ported from [go-raknet](https://github.com/Sandertv/go-raknet).

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
ragnet = "0.1.0"
tokio = { version = "1", features = ["full"] }
```

## Quick Start

### Server

```rust
use ragnet::{Listener, Result};

#[tokio::main]
async fn main() -> Result<()> {
    // Bind to all interfaces (IPv4 + IPv6)
    let listener = Listener::bind("[::]:19132").await?;

    println!("Server listening on port 19132");

    loop {
        let conn = listener.accept().await?;

        tokio::spawn(async move {
            let mut buf = vec![0u8; 4 * 1024 * 1024];

            loop {
                match conn.read(&mut buf).await {
                    Ok(n) if n > 0 => {
                        println!("Received {} bytes", n);
                        // Echo back
                        let _ = conn.write(&buf[..n]).await;
                    }
                    _ => break,
                }
            }

            let _ = conn.close().await;
        });
    }
}
```

### Client

```rust
use ragnet::{Dialer, Result};

#[tokio::main]
async fn main() -> Result<()> {
    // Connect to a server
    let conn = Dialer::new().dial("127.0.0.1:19132").await?;

    // Send data
    conn.write(b"Hello, server!").await?;

    // Receive response
    let mut buf = vec![0u8; 4 * 1024 * 1024];
    let n = conn.read(&mut buf).await?;
    println!("Received: {:?}", &buf[..n]);

    // Close connection
    conn.close().await?;
    
    Ok(())
}
```

### Ping a Server

```rust
use ragnet::dial::ping;

#[tokio::main]
async fn main() {
    match ping("play.example.com:19132").await {
        Ok(data) => {
            println!("Server info: {}", String::from_utf8_lossy(&data));
        }
        Err(e) => {
            println!("Server offline: {}", e);
        }
    }
}
```
