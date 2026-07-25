use std::sync::Arc;

/// Abstract transport layer for DAG consensus.
/// Default implementation uses TCP. Swap for DPDK, satellite, radio, etc.
pub trait Transport: Send + Sync {
    /// Bind and listen for incoming connections on `addr`.
    /// For each received message (raw bytes), calls `on_message(peer_addr, data)`.
    fn listen(&self, addr: &str, on_message: Arc<dyn Fn(Vec<u8>, String) + Send + Sync>);

    /// Send raw bytes to a peer address.
    fn send(&self, addr: &str, data: &[u8]) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>>;
}

/// Abstract timestamp source for the hot path.
/// Default: chrono::Utc::now(). Swap for PTP IEEE 1588 hardware stamps.
pub trait TimestampSource: Send + Sync {
    fn now_ns(&self) -> i64;
}

// ======================================================================
// Default TCP Implementation
// ======================================================================

pub struct TcpTransport;

impl Transport for TcpTransport {
    fn listen(&self, listen_addr: &str, on_message: Arc<dyn Fn(Vec<u8>, String) + Send + Sync>) {
        use tokio::io::AsyncReadExt;
        use tokio::net::TcpListener;
        let addr = listen_addr.to_string();
        tokio::spawn(async move {
            let listener = match TcpListener::bind(&addr).await {
                Ok(l) => l,
                Err(e) => {
                    tracing::error!(addr = %addr, error = %e, "tcp transport: bind failed");
                    return;
                }
            };
            tracing::info!(addr = %addr, "tcp transport: listening");
            loop {
                match listener.accept().await {
                    Ok((mut stream, peer)) => {
                        let cb = on_message.clone();
                        tokio::spawn(async move {
                            let peer_str = peer.to_string();
                            let mut len_buf = [0u8; 8];
                            if stream.read_exact(&mut len_buf).await.is_err() { return; }
                            let len = u64::from_le_bytes(len_buf) as usize;
                            let mut buf = vec![0u8; len];
                            if stream.read_exact(&mut buf).await.is_err() { return; }
                            cb(buf, peer_str);
                        });
                    }
                    Err(_) => {}
                }
            }
        });
    }

    fn send(&self, addr: &str, data: &[u8]) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>> {
        use tokio::io::AsyncWriteExt;
        let addr = addr.to_string();
        let data = data.to_vec();
        Box::pin(async move {
            let mut stream = tokio::time::timeout(
                std::time::Duration::from_millis(100),
                tokio::net::TcpStream::connect(&addr),
            ).await.map_err(|_| "timeout".to_string())?
              .map_err(|e| format!("connect: {}", e))?;
            let len = (data.len() as u64).to_le_bytes();
            stream.write_all(&len).await.map_err(|e| format!("send len: {}", e))?;
            stream.write_all(&data).await.map_err(|e| format!("send data: {}", e))?;
            stream.flush().await.map_err(|e| format!("flush: {}", e))?;
            Ok(())
        })
    }
}

// ======================================================================
// Default Timestamp Implementation
// ======================================================================

pub struct SystemTimestampSource;

impl TimestampSource for SystemTimestampSource {
    fn now_ns(&self) -> i64 {
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    }
}

pub const TIMESTAMP_SOURCE: SystemTimestampSource = SystemTimestampSource;
pub const TCP_TRANSPORT: TcpTransport = TcpTransport;
