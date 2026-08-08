use compact_str::CompactString;
use crate::orderbook::OrderBookManager;
use crate::types::*;
use dashmap::DashMap;
use futures::future::BoxFuture;
use rustls_pemfile::{certs, pkcs8_private_keys};
use rustls::ServerConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::BufReader;
use tokio_rustls::TlsAcceptor;
use std::sync::Arc;
use tokio::net::{TcpListener};
use tokio::sync::mpsc;
use uuid::Uuid;

pub type ProcessOrderFn = Arc<dyn Fn(Order) -> BoxFuture<'static, Result<PlaceOrderResult, String>> + Send + Sync>;

const SOH: u8 = 0x01;
const HEARTBEAT_SECS: u64 = 30;
const FIX_DEFAULT_PORT: u16 = 4001;
const FIX_MAX_MSGS_PER_SEC: u64 = 1000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FIXSessionInfo {
    pub session_id: String,
    pub institution: String,
    pub sender_comp_id: String,
    pub target_comp_id: String,
    pub connected: bool,
    pub seq_num_in: u64,
    pub seq_num_out: u64,
    pub last_heartbeat: i64,
    pub orders_routed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FIXMessage {
    pub msg_type: String,
    pub msg_seq_num: u64,
    pub sender_comp_id: String,
    pub target_comp_id: String,
    pub sending_time: i64,
    pub body: HashMap<String, String>,
    pub raw: String,
}

impl FIXMessage {
    pub fn parse(raw: &[u8]) -> Option<Self> {
        let s = std::str::from_utf8(raw).ok()?;
        let mut tags: HashMap<String, String> = HashMap::new();
        for pair in s.split(SOH as char) {
            if pair.is_empty() { continue; }
            let mut parts = pair.splitn(2, '=');
            let tag = parts.next()?.to_string();
            let value = parts.next()?.to_string();
            tags.insert(tag, value);
        }
        let msg_type = tags.get("35")?.clone();
        let msg_seq_num = tags.get("34")?.parse().ok()?;
        let sender_comp_id = tags.get("49")?.clone();
        let target_comp_id = tags.get("56")?.clone();
        let sending_time_str = tags.get("52")?;
        let sending_time = sending_time_str.parse::<i64>().unwrap_or(0);

        Some(FIXMessage {
            msg_type,
            msg_seq_num,
            sender_comp_id,
            target_comp_id,
            sending_time,
            body: tags,
            raw: String::from_utf8_lossy(raw).to_string(),
        })
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut parts: Vec<String> = Vec::new();
        parts.push(format!("8=FIX.5.0SP2"));
        parts.push(format!("35={}", self.msg_type));
        parts.push(format!("34={}", self.msg_seq_num));
        parts.push(format!("49={}", self.sender_comp_id));
        parts.push(format!("56={}", self.target_comp_id));
        parts.push(format!("52={}", self.sending_time));
        for (k, v) in &self.body {
            parts.push(format!("{}={}", k, v));
        }
        let mut raw = parts.join(&String::from(SOH as char));
        raw.push(SOH as char);
        let sum: u8 = raw.bytes().fold(0u8, |acc, b| acc.wrapping_add(b));
        raw.push_str(&format!("10={:03}", sum));
        raw.push(SOH as char);
        raw.into_bytes()
    }
}

fn heartbeat_msg(sender: &str, target: &str, seq: u64) -> FIXMessage {
    FIXMessage {
        msg_type: "0".into(),
        msg_seq_num: seq,
        sender_comp_id: sender.into(),
        target_comp_id: target.into(),
        sending_time: chrono::Utc::now().timestamp_millis(),
        body: HashMap::new(),
        raw: String::new(),
    }
}

fn logon_msg(sender: &str, target: &str, seq: u64, heartbeat_secs: u64) -> FIXMessage {
    let mut body = HashMap::new();
    body.insert("98".into(), "0".into());
    body.insert("108".into(), heartbeat_secs.to_string());
    body.insert("141".into(), "Y".into());
    body.insert("553".into(), sender.into());
    body.insert("554".into(), "password_placeholder".into());
    FIXMessage {
        msg_type: "A".into(),
        msg_seq_num: seq,
        sender_comp_id: sender.into(),
        target_comp_id: target.into(),
        sending_time: chrono::Utc::now().timestamp_millis(),
        body,
        raw: String::new(),
    }
}

fn execution_report(
    seq: u64,
    sender: &str,
    target: &str,
    order: &Order,
    trade: Option<&Trade>,
    exec_type: &str,
) -> FIXMessage {
    let mut body = HashMap::new();
    body.insert("37".into(), order.id.to_string());
    body.insert("11".into(), order.id.to_string());
    body.insert("17".into(), Uuid::new_v4().to_string());
    body.insert("150".into(), exec_type.into());
    body.insert("39".into(), match order.status {
        OrderStatus::New => "0",
        OrderStatus::PartiallyFilled => "1",
        OrderStatus::Filled => "2",
        OrderStatus::Cancelled => "4",
        OrderStatus::Rejected => "8",
        OrderStatus::Expired => "C",
    }.into());
    body.insert("54".into(), match order.side {
        OrderSide::Buy => "1",
        OrderSide::Sell => "2",
    }.into());
    body.insert("38".into(), order.quantity.to_string());
    body.insert("44".into(), order.price.to_string());
    body.insert("32".into(), order.filled.to_string());
    body.insert("31".into(), if let Some(t) = trade { t.price.to_string() } else { "0".into() });
    body.insert("14".into(), order.filled.to_string());
    body.insert("151".into(), order.remaining.to_string());
    body.insert("60".into(), chrono::Utc::now().timestamp_millis().to_string());
    FIXMessage {
        msg_type: "8".into(),
        msg_seq_num: seq,
        sender_comp_id: sender.into(),
        target_comp_id: target.into(),
        sending_time: chrono::Utc::now().timestamp_millis(),
        body,
        raw: String::new(),
    }
}

enum FIXCommand {
    Send(FIXMessage),
    Disconnect(String),
}

struct FIXSessionState {
    info: FIXSessionInfo,
    sender: mpsc::UnboundedSender<FIXCommand>,
    authenticated: bool,
    orders_this_second: u64,
    rate_window_start: std::time::Instant,
}

pub struct FIXGateway {
    sessions: DashMap<String, FIXSessionState>,
    total_orders_routed: std::sync::atomic::AtomicU64,
    book_manager: Arc<OrderBookManager>,
    port: u16,
    tls_port: Option<u16>,
    tls_acceptor: Option<TlsAcceptor>,
    order_fn: Option<ProcessOrderFn>,
}

impl FIXGateway {
    pub fn new(book_manager: Arc<OrderBookManager>) -> Self {
        let tls_acceptor = Self::load_tls_config().ok().flatten();
        let tls_port = if tls_acceptor.is_some() { Some(4443) } else { None };
        Self {
            sessions: DashMap::new(),
            total_orders_routed: std::sync::atomic::AtomicU64::new(0),
            book_manager,
            port: FIX_DEFAULT_PORT,
            tls_port,
            tls_acceptor,
            order_fn: None,
        }
    }

    pub fn set_order_fn(&mut self, f: ProcessOrderFn) {
        self.order_fn = Some(f);
    }

    fn load_tls_config() -> Result<Option<TlsAcceptor>, String> {
        let cert_path = match std::env::var("FIX_TLS_CERT") {
            Ok(p) => p,
            Err(_) => return Ok(None),
        };
        let key_path = std::env::var("FIX_TLS_KEY")
            .map_err(|_| "FIX_TLS_KEY required when FIX_TLS_CERT is set".to_string())?;

        let cert_file = std::fs::File::open(&cert_path)
            .map_err(|e| format!("FIX TLS cert open: {}", e))?;
        let key_file = std::fs::File::open(&key_path)
            .map_err(|e| format!("FIX TLS key open: {}", e))?;

        let cert_chain = certs(&mut BufReader::new(cert_file))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("FIX TLS cert parse: {}", e))?;
        let mut keys = pkcs8_private_keys(&mut BufReader::new(key_file))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("FIX TLS key parse: {}", e))?;

        if keys.is_empty() {
            return Err("FIX TLS: no private keys found".into());
        }

        let key = keys.remove(0).into();

        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(cert_chain, key)
            .map_err(|e| format!("FIX TLS config: {}", e))?;

        Ok(Some(TlsAcceptor::from(Arc::new(config))))
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    pub async fn start(&self) {
        if self.tls_acceptor.is_none() {
            tracing::warn!("FIX gateway running WITHOUT TLS — connections are plaintext. Set FIX_TLS_CERT and FIX_TLS_KEY for production.");
        }
        let addr = format!("0.0.0.0:{}", self.port);
        let listener = match TcpListener::bind(&addr).await {
            Ok(l) => l,
            Err(e) => {
                tracing::error!(port = self.port, error = %e, "FIX: failed to bind");
                return;
            }
        };
        tracing::info!(port = self.port, "FIX/FAST 5.0 SP2 gateway listening");

        let gateway = Arc::new(self.clone_inner());

        // Spawn plain TCP acceptor
        let g_tcp = gateway.clone();
        let tcp_handle = tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, peer)) => {
                        tracing::info!(peer = %peer, port = g_tcp.port, "FIX: new connection");
                        let g = g_tcp.clone();
                        tokio::spawn(async move {
                            g.handle_session(stream, peer).await;
                        });
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "FIX: accept failed");
                    }
                }
            }
        });

        // Spawn TLS acceptor if configured
        if let Some(tls_acceptor) = &self.tls_acceptor {
            let tls_port = self.tls_port.unwrap_or(4443);
            let tls_addr = format!("0.0.0.0:{}", tls_port);
            let tls_listener = match TcpListener::bind(&tls_addr).await {
                Ok(l) => l,
                Err(e) => {
                    tracing::error!(port = tls_port, error = %e, "FIX/TLS: failed to bind");
                    return;
                }
            };
            tracing::info!(port = tls_port, "FIX/FAST 5.0 SP2 TLS gateway listening");

            let g_tls = gateway.clone();
            let acceptor = tls_acceptor.clone();
            tokio::spawn(async move {
                loop {
                    match tls_listener.accept().await {
                        Ok((stream, peer)) => {
                            tracing::info!(peer = %peer, port = tls_port, "FIX/TLS: new connection");
                            let g = g_tls.clone();
                            let acc = acceptor.clone();
                            tokio::spawn(async move {
                                match acc.accept(stream).await {
                                    Ok(tls_stream) => {
                                        g.handle_session(tls_stream, peer).await;
                                    }
                                    Err(e) => {
                                        tracing::error!(peer = %peer, error = %e, "FIX/TLS: handshake failed");
                                    }
                                }
                            });
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "FIX/TLS: accept failed");
                        }
                    }
                }
            });
        }

        // Keep main task alive
        tcp_handle.await.ok();
    }

    fn clone_inner(&self) -> Self {
        Self {
            sessions: DashMap::new(),
            total_orders_routed: std::sync::atomic::AtomicU64::new(
                self.total_orders_routed.load(std::sync::atomic::Ordering::Relaxed)
            ),
            book_manager: self.book_manager.clone(),
            port: self.port,
            tls_port: self.tls_port,
            tls_acceptor: self.tls_acceptor.clone(),
            order_fn: self.order_fn.clone(),
        }
    }

    async fn handle_session<S>(&self, stream: S, peer: std::net::SocketAddr)
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Unpin + 'static,
    {
        let (read, write) = tokio::io::split(stream);
        let (tx, rx) = mpsc::unbounded_channel::<FIXCommand>();
        let mut writer = tokio::io::BufWriter::new(write);
        let mut reader = tokio::io::BufReader::new(read);

        let session_id = format!("FIX_{}", Uuid::new_v4().to_string()[..8].to_uppercase());
        let info = FIXSessionInfo {
            session_id: session_id.clone(),
            institution: format!("INST_{}", peer.port()),
            sender_comp_id: "THE-BRIDGE".into(),
            target_comp_id: session_id.clone(),
            connected: true,
            seq_num_in: 1,
            seq_num_out: 1,
            last_heartbeat: chrono::Utc::now().timestamp_millis(),
            orders_routed: 0,
        };

        self.sessions.insert(session_id.clone(), FIXSessionState {
            info: info.clone(),
            sender: tx.clone(),
            authenticated: false,
            orders_this_second: 0,
            rate_window_start: std::time::Instant::now(),
        });

        let gateway = self.clone_inner();
        let sid = session_id.clone();
        let sid2 = sid.clone();

        let writer_task = tokio::spawn(async move {
            use tokio::io::AsyncWriteExt;
            let mut rx = rx;
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    FIXCommand::Send(msg) => {
                        let encoded = msg.encode();
                        if let Err(e) = writer.write_all(&encoded).await {
                            tracing::warn!(session = %sid, error = %e, "FIX: write error");
                            break;
                        }
                        if let Err(e) = writer.flush().await {
                            tracing::warn!(session = %sid, error = %e, "FIX: flush error");
                            break;
                        }
                    }
                    FIXCommand::Disconnect(reason) => {
                        tracing::info!(session = %sid, reason = %reason, "FIX: disconnecting");
                        let _ = writer.shutdown().await;
                        break;
                    }
                }
            }
        });

        let reader_task = tokio::spawn(async move {
            use tokio::io::AsyncBufReadExt;
            let mut buf = Vec::with_capacity(4096);
            loop {
                let n = match reader.read_until(SOH, &mut buf).await {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(e) => {
                        tracing::warn!(session = %sid2, error = %e, "FIX: read error");
                        break;
                    }
                };
                if n < 10 { continue; }
                if buf.len() < 4 { continue; }
                if &buf[buf.len()-4..buf.len()-1] != b"\x0110=" {
                    continue;
                }
                let msg = match FIXMessage::parse(&buf) {
                    Some(m) => m,
                    None => {
                        buf.clear();
                        continue;
                    }
                };
                buf.clear();

                gateway.handle_message(&sid2, msg).await;
            }
        });

        let _ = tokio::join!(writer_task, reader_task);
        self.sessions.remove(&session_id);
        tracing::info!(session = %session_id, "FIX: session ended");
    }

    async fn handle_message(&self, session_id: &str, msg: FIXMessage) {
        match msg.msg_type.as_str() {
            "A" => {
                tracing::info!(session = %session_id, sender = %msg.sender_comp_id, "FIX: Logon");
                if let Some(mut session) = self.sessions.get_mut(session_id) {
                    session.authenticated = true;
                }
                let logon_ack = logon_msg(
                    "THE-BRIDGE",
                    &msg.sender_comp_id,
                    1,
                    HEARTBEAT_SECS,
                );
                if let Some(session) = self.sessions.get(session_id) {
                    let _ = session.sender.send(FIXCommand::Send(logon_ack));
                }
            }
            "0" => {
                if let Some(mut session) = self.sessions.get_mut(session_id) {
                    session.info.last_heartbeat = chrono::Utc::now().timestamp_millis();
                }
                let hb = heartbeat_msg("THE-BRIDGE", &msg.sender_comp_id, 0);
                if let Some(session) = self.sessions.get(session_id) {
                    let _ = session.sender.send(FIXCommand::Send(hb));
                }
            }
            "5" => {
                tracing::info!(session = %session_id, "FIX: Logout");
                if let Some(session) = self.sessions.get(session_id) {
                    let _ = session.sender.send(FIXCommand::Disconnect("remote logout".into()));
                }
            }
            "D" | "F" => {
                if let Some(session) = self.sessions.get(session_id) {
                    if !session.authenticated {
                        self.send_reject(session_id, &msg.body.get("11").cloned().unwrap_or_default(), "Not authenticated — send Logon first").await;
                        return;
                    }
                    {
                        let mut session = self.sessions.get_mut(session_id).unwrap();
                        let now = std::time::Instant::now();
                        if now.duration_since(session.rate_window_start).as_secs() >= 1 {
                            session.orders_this_second = 0;
                            session.rate_window_start = now;
                        }
                        session.orders_this_second += 1;
                        if session.orders_this_second > FIX_MAX_MSGS_PER_SEC {
                            self.send_reject(session_id, &msg.body.get("11").cloned().unwrap_or_default(), "Rate limit exceeded").await;
                            return;
                        }
                    }
                    if let Some(session) = self.sessions.get(session_id) {
                        if session.info.seq_num_in > 1 && msg.msg_seq_num != session.info.seq_num_in {
                            if msg.msg_seq_num < session.info.seq_num_in {
                                self.send_reject(session_id, &msg.body.get("11").cloned().unwrap_or_default(), "Sequence number too low — possible replay").await;
                                return;
                            }
                        }
                    }
                    if let Some(mut session) = self.sessions.get_mut(session_id) {
                        session.info.seq_num_in = msg.msg_seq_num + 1;
                    }
                }
                if msg.msg_type == "D" {
                    self.handle_new_order(session_id, &msg).await;
                } else {
                    self.handle_cancel(session_id, &msg).await;
                }
            }
            "2" => {
                tracing::warn!(session = %session_id, seq = %msg.msg_seq_num, "FIX: ResendRequest — full recovery not yet implemented");
            }
            _ => {
                tracing::warn!(session = %session_id, type = %msg.msg_type, "FIX: unknown message type");
            }
        }
    }

    async fn handle_new_order(&self, session_id: &str, msg: &FIXMessage) {
        let cl_ord_id = msg.body.get("11").cloned().unwrap_or_default();
        let symbol = msg.body.get("55").cloned().unwrap_or("USD/EGP".into());
        let side = match msg.body.get("54").map(|s| s.as_str()) {
            Some("1") => OrderSide::Buy,
            Some("2") => OrderSide::Sell,
            _ => {
                self.send_reject(session_id, &cl_ord_id, "Invalid Side (54)").await;
                return;
            }
        };
        let order_qty: f64 = match msg.body.get("38").and_then(|v| v.parse().ok()) {
            Some(q) if q > 0.0 => q,
            _ => {
                self.send_reject(session_id, &cl_ord_id, "Invalid OrderQty (38)").await;
                return;
            }
        };
        let price: f64 = msg.body.get("44").and_then(|v| v.parse().ok()).unwrap_or(0.0);
        let ord_type = msg.body.get("40").map(|s| s.as_str());
        let order_type = match ord_type {
            Some("2") => OrderType::Limit,
            Some("1") => OrderType::Market,
            _ => {
                self.send_reject(session_id, &cl_ord_id, "Unsupported OrdType (40)").await;
                return;
            }
        };

        let order = Order {
            id: Uuid::new_v4(),
            id_tag: 0,
            user_id: Uuid::nil(),
            pair: CompactString::from(symbol.to_uppercase()),
            order_type,
            side,
            price,
            quantity: order_qty,
            filled: 0.0,
            remaining: order_qty,
            status: OrderStatus::New,
            timestamp: chrono::Utc::now().timestamp_millis(),
            ttl_ms: None,
            is_swap: false,
            swap_target_currency: None,
            tee_signed: false,
            dot_verified: false,
            stealth: false,
            trailing_offset: None,
            trigger_price: None,
            hard_floor: None,
            track: crate::types::Track::Compliant,
            style: crate::types::OrderStyle::Standard,
            hidden_remaining: 0.0,
            client_order_id: None,
            filled_quantity: 0,
        };

        self.total_orders_routed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            session.info.orders_routed += 1;
        }

        let result = if let Some(ref pipeline) = self.order_fn {
            pipeline(order.clone()).await
        } else {
            self.book_manager.place_order(order.clone()).map_err(|e| e.to_string())
        };

        match result {
            Ok(pr) => {
                let exec_type = match pr.order.status {
                    OrderStatus::Filled => "2",
                    OrderStatus::PartiallyFilled => "1",
                    _ => "0",
                };
                let er = execution_report(
                    self.next_seq_out(session_id),
                    "THE-BRIDGE",
                    &msg.sender_comp_id,
                    &pr.order,
                    pr.trades.first(),
                    exec_type,
                );
                if let Some(session) = self.sessions.get(session_id) {
                    let _ = session.sender.send(FIXCommand::Send(er));
                }
                for trade in &pr.trades {
                    let fill_er = execution_report(
                        self.next_seq_out(session_id),
                        "THE-BRIDGE",
                        &msg.sender_comp_id,
                        &pr.order,
                        Some(trade),
                        "1",
                    );
                    if let Some(session) = self.sessions.get(session_id) {
                        let _ = session.sender.send(FIXCommand::Send(fill_er));
                    }
                }
            }
            Err(e) => {
                self.send_reject(session_id, &cl_ord_id, &e).await;
            }
        }
    }

    async fn handle_cancel(&self, session_id: &str, msg: &FIXMessage) {
        let orig_cl_ord_id = msg.body.get("41").cloned().unwrap_or_default();
        let order_id = Uuid::parse_str(&orig_cl_ord_id).unwrap_or_default();
        match self.book_manager.cancel_order(order_id) {
            Ok(_) => {
                let er = execution_report(
                    self.next_seq_out(session_id),
                    "THE-BRIDGE",
                    &msg.sender_comp_id,
                    &Order {
                        id: order_id,
                        status: OrderStatus::Cancelled,
                        ..Default::default()
                    },
                    None,
                    "4",
                );
                if let Some(session) = self.sessions.get(session_id) {
                    let _ = session.sender.send(FIXCommand::Send(er));
                }
            }
            Err(e) => {
                self.send_reject(session_id, &orig_cl_ord_id, &e.to_string()).await;
            }
        }
    }

    async fn send_reject(&self, session_id: &str, cl_ord_id: &str, reason: &str) {
        let mut body = HashMap::new();
        body.insert("11".into(), cl_ord_id.into());
        body.insert("58".into(), reason.into());
        body.insert("371".into(), "35".into());
        body.insert("372".into(), "D".into());
        body.insert("373".into(), "99".into());
        let reject = FIXMessage {
            msg_type: "3".into(),
            msg_seq_num: self.next_seq_out(session_id),
            sender_comp_id: "THE-BRIDGE".into(),
            target_comp_id: session_id.into(),
            sending_time: chrono::Utc::now().timestamp_millis(),
            body,
            raw: String::new(),
        };
        if let Some(session) = self.sessions.get(session_id) {
            let _ = session.sender.send(FIXCommand::Send(reject));
        }
    }

    fn next_seq_out(&self, session_id: &str) -> u64 {
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            let seq = session.info.seq_num_out;
            session.info.seq_num_out += 1;
            seq
        } else { 1 }
    }

    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    pub fn connected_institutions(&self) -> Vec<String> {
        self.sessions.iter().map(|s| s.info.institution.clone()).collect()
    }

    pub fn total_orders_routed(&self) -> u64 {
        self.total_orders_routed.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn sessions(&self) -> Vec<FIXSessionInfo> {
        self.sessions.iter().map(|s| s.info.clone()).collect()
    }
}
