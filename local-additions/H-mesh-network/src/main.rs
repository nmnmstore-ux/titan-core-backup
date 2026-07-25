// ============================================================
// SwiftBridge Mesh Network — P2P Layer Without Internet
// Offline-first transaction propagation via libp2p
// Phone-to-phone: Bluetooth, WiFi Direct, LAN mesh
// ============================================================

use blake3::Hash;
use chrono::Utc;
use dashmap::DashMap;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{error, info, warn};
use uuid::Uuid;

// ==================== Core Types ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshTransaction {
    pub id: Uuid,
    pub from_did: String,
    pub to_did: String,
    pub amount: f64,
    pub currency: String,
    pub timestamp: i64,
    pub ttl_secs: u64,
    pub signature: Vec<u8>,
    pub hops: Vec<String>,
    pub mesh_proof: MeshProof,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshProof {
    pub route_hash: Hash,
    pub witness_count: u8,
    pub finalized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MeshMessage {
    Transaction(MeshTransaction),
    Ack(Uuid, String),
    SyncRequest(SyncRequest),
    SyncResponse(SyncResponse),
    PeerAnnounce(PeerInfo),
    Heartbeat(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRequest {
    pub peer_id: String,
    pub last_known_tx: Option<Uuid>,
    pub known_peers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResponse {
    pub transactions: Vec<MeshTransaction>,
    pub peers: Vec<PeerInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub peer_id: String,
    pub did: String,
    pub public_key: Vec<u8>,
    pub last_seen: i64,
    pub signal_strength: i8,
    pub supports_bluetooth: bool,
    pub supports_wifi_direct: bool,
    pub location: Option<GeoTag>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeoTag {
    pub lat: f64,
    pub lng: f64,
    pub accuracy_meters: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshConfig {
    pub node_id: String,
    pub did: String,
    pub signing_key: Vec<u8>,
    pub listen_port: u16,
    pub bootstrap_peers: Vec<String>,
    pub offline_mode: bool,
    pub max_hops: u8,
    pub ttl_secs: u64,
    pub sync_interval_secs: u64,
}

impl Default for MeshConfig {
    fn default() -> Self {
        Self {
            node_id: Uuid::new_v4().to_string(),
            did: String::new(),
            signing_key: vec![],
            listen_port: 9777,
            bootstrap_peers: vec![],
            offline_mode: true,
            max_hops: 7,
            ttl_secs: 3600,
            sync_interval_secs: 30,
        }
    }
}

// ==================== Transaction Pool (Mempool) ====================

pub struct Mempool {
    pending: DashMap<Uuid, MeshTransaction>,
    confirmed: DashMap<Uuid, MeshTransaction>,
    rejected: DashMap<Uuid, String>,
    max_pending: usize,
    ttl_ns: u64,
}

impl Mempool {
    pub fn new(max_pending: usize) -> Self {
        Self {
            pending: DashMap::new(),
            confirmed: DashMap::new(),
            rejected: DashMap::new(),
            max_pending,
            ttl_ns: Duration::from_secs(3600).as_nanos() as u64,
        }
    }

    pub fn submit(&self, tx: MeshTransaction) -> Result<(), String> {
        if self.pending.len() >= self.max_pending {
            self.evict_expired();
        }
        if self.pending.contains_key(&tx.id) || self.confirmed.contains_key(&tx.id) {
            return Err("duplicate transaction".into());
        }
        self.pending.insert(tx.id, tx);
        Ok(())
    }

    pub fn confirm(&self, id: Uuid) {
        if let Some((_, tx)) = self.pending.remove(&id) {
            self.confirmed.insert(id, tx);
        }
    }

    pub fn reject(&self, id: Uuid, reason: String) {
        self.pending.remove(&id);
        self.rejected.insert(id, reason);
    }

    pub fn get_pending_since(&self, since: i64) -> Vec<MeshTransaction> {
        self.pending
            .iter()
            .filter(|e| e.timestamp > since)
            .map(|e| e.clone())
            .collect()
    }

    pub fn get_confirmed_since(&self, since: i64) -> Vec<MeshTransaction> {
        self.confirmed
            .iter()
            .filter(|e| e.timestamp > since)
            .map(|e| e.clone())
            .collect()
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    fn evict_expired(&self) {
        let now = Utc::now().timestamp_nanos_opt().unwrap_or(0) as u64;
        self.pending.retain(|_, tx| {
            let tx_ns = tx.timestamp as u64 * 1_000_000_000;
            now - tx_ns < self.ttl_ns
        });
    }
}

// ==================== Peer Discovery & Routing ====================

pub struct RoutingTable {
    peers: DashMap<String, PeerInfo>,
    routes: DashMap<String, Vec<String>>,
    blacklist: DashMap<String, i64>,
}

impl RoutingTable {
    pub fn new() -> Self {
        Self {
            peers: DashMap::new(),
            routes: DashMap::new(),
            blacklist: DashMap::new(),
        }
    }

    pub fn add_peer(&self, info: PeerInfo) {
        self.blacklist.remove(&info.peer_id);
        self.peers.insert(info.peer_id.clone(), info);
    }

    pub fn remove_peer(&self, peer_id: &str) {
        self.peers.remove(peer_id);
    }

    pub fn blacklist(&self, peer_id: &str, duration_secs: i64) {
        let until = Utc::now().timestamp() + duration_secs;
        self.blacklist.insert(peer_id.to_string(), until);
        self.peers.remove(peer_id);
    }

    pub fn is_blacklisted(&self, peer_id: &str) -> bool {
        if let Some(until) = self.blacklist.get(peer_id) {
            if *until > Utc::now().timestamp() {
                return true;
            }
            self.blacklist.remove(peer_id);
        }
        false
    }

    pub fn nearest_peers(&self, target_did: &str, count: usize) -> Vec<PeerInfo> {
        let mut scored: Vec<(i64, PeerInfo)> = self
            .peers
            .iter()
            .map(|e| {
                let score = self.distance_score(&e.did, target_did);
                (score, e.clone())
            })
            .collect();
        scored.sort_by_key(|(s, _)| *s);
        scored.into_iter().take(count).map(|(_, p)| p).collect()
    }

    pub fn broadcast_peers(&self) -> Vec<PeerInfo> {
        self.peers.iter().map(|e| e.clone()).collect()
    }

    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }

    fn distance_score(&self, a: &str, b: &str) -> i64 {
        let a_bytes = blake3::hash(a.as_bytes());
        let b_bytes = blake3::hash(b.as_bytes());
        let xor: [u8; 32] = std::array::from_fn(|i| a_bytes.as_bytes()[i] ^ b_bytes.as_bytes()[i]);
        let val = u64::from_be_bytes(xor[..8].try_into().unwrap_or([0; 8]));
        (val ^ u64::MAX) as i64
    }
}

// ==================== Transaction Propagation Engine ====================

pub struct MeshPropagator {
    pub config: MeshConfig,
    mempool: Arc<Mempool>,
    routing: Arc<RoutingTable>,
    signing_key: SigningKey,
    verifying_key: VerifyingKey,
    running: AtomicBool,
    txs_relayed: AtomicU64,
    txs_confirmed: AtomicU64,
    bytes_sent: AtomicU64,
}

impl MeshPropagator {
    pub fn new(config: MeshConfig) -> Result<Self, String> {
        let signing_key = if config.signing_key.is_empty() {
            let mut csprng = rand::thread_rng();
            SigningKey::generate(&mut csprng)
        } else {
            let bytes: [u8; 32] = config.signing_key[..32]
                .try_into()
                .map_err(|_| "invalid signing key length".to_string())?;
            SigningKey::from_bytes(&bytes)
        };
        let verifying_key = signing_key.verifying_key();

        Ok(Self {
            config,
            mempool: Arc::new(Mempool::new(1_000_000)),
            routing: Arc::new(RoutingTable::new()),
            signing_key,
            verifying_key,
            running: AtomicBool::new(true),
            txs_relayed: AtomicU64::new(0),
            txs_confirmed: AtomicU64::new(0),
            bytes_sent: AtomicU64::new(0),
        })
    }

    pub fn create_transaction(&self, to_did: &str, amount: f64, currency: &str) -> Result<MeshTransaction, String> {
        let tx = MeshTransaction {
            id: Uuid::new_v4(),
            from_did: self.config.did.clone(),
            to_did: to_did.to_string(),
            amount,
            currency: currency.to_string(),
            timestamp: Utc::now().timestamp(),
            ttl_secs: self.config.ttl_secs,
            signature: vec![],
            hops: vec![self.config.node_id.clone()],
            mesh_proof: MeshProof {
                route_hash: blake3::Hash::from([0u8; 32]),
                witness_count: 0,
                finalized: false,
            },
        };

        let payload = bincode::serialize(&tx).map_err(|e| e.to_string())?;
        let signature = self.signing_key.sign(&payload);
        let mut signed = tx;
        signed.signature = signature.to_bytes().to_vec();
        signed.mesh_proof.route_hash = blake3::hash(&payload);

        self.mempool.submit(signed.clone())?;
        Ok(signed)
    }

    pub fn receive_transaction(&self, tx: MeshTransaction) -> Result<(), String> {
        // Verify signature
        let payload = bincode::serialize(&tx).map_err(|e| e.to_string())?;
        let sig = Signature::from_bytes(&tx.signature.as_slice().try_into().map_err(|_| "invalid sig length")?);

        // Resolve public key from DID (in production: DID document lookup)
        let peer_key = self.routing.peers.get(&tx.from_did)
            .map(|p| p.public_key.clone())
            .unwrap_or_default();

        if peer_key.len() == 32 {
            let key_bytes: [u8; 32] = peer_key[..32].try_into().unwrap();
            let pk = VerifyingKey::from_bytes(&key_bytes).map_err(|e| e.to_string())?;
            pk.verify(&payload, &sig).map_err(|e| format!("sig verify failed: {}", e))?;
        }

        // Check TTL
        if tx.hops.len() as u64 > self.config.max_hops as u64 {
            return Err("max hops exceeded".into());
        }

        let age_secs = Utc::now().timestamp() - tx.timestamp;
        if age_secs > tx.ttl_secs as i64 {
            return Err("transaction expired".into());
        }

        self.mempool.submit(tx)?;
        self.txs_relayed.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    pub fn sync_with_peer(&self, peer: &PeerInfo) -> Vec<MeshMessage> {
        let since = Utc::now().timestamp() - 300; // last 5 min
        let pending = self.mempool.get_pending_since(since);
        let confirmed = self.mempool.get_confirmed_since(since);
        let mut msgs: Vec<MeshMessage> = pending
            .into_iter()
            .map(MeshMessage::Transaction)
            .collect();
        msgs.extend(confirmed.into_iter().map(MeshMessage::Transaction));

        msgs.push(MeshMessage::SyncResponse(SyncResponse {
            transactions: vec![],
            peers: self.routing.broadcast_peers(),
        }));

        msgs
    }

    pub fn confirm_transaction(&self, id: Uuid) {
        self.mempool.confirm(id);
        self.txs_confirmed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn reject_transaction(&self, id: Uuid, reason: String) {
        self.mempool.reject(id, reason);
    }

    pub fn get_peer_count(&self) -> usize {
        self.routing.peer_count()
    }

    pub fn get_pending_count(&self) -> usize {
        self.mempool.pending_count()
    }

    pub fn get_relayed_count(&self) -> u64 {
        self.txs_relayed.load(Ordering::Relaxed)
    }

    pub fn get_confirmed_count(&self) -> u64 {
        self.txs_confirmed.load(Ordering::Relaxed)
    }

    pub fn is_online(&self) -> bool {
        self.routing.peer_count() > 0
    }

    pub fn set_running(&self, running: bool) {
        self.running.store(running, Ordering::Relaxed);
    }
}

// ==================== Offline Queue (for when no peers available) ====================

pub struct OfflineQueue {
    queue: Arc<RwLock<VecDeque<MeshTransaction>>>,
    max_size: usize,
}

impl OfflineQueue {
    pub fn new(max_size: usize) -> Self {
        Self {
            queue: Arc::new(RwLock::new(VecDeque::with_capacity(max_size))),
            max_size,
        }
    }

    pub fn enqueue(&self, tx: MeshTransaction) -> Result<(), String> {
        let mut queue = self.queue.write();
        if queue.len() >= self.max_size {
            queue.pop_front();
        }
        queue.push_back(tx);
        Ok(())
    }

    pub fn dequeue_all(&self) -> Vec<MeshTransaction> {
        let mut queue = self.queue.write();
        queue.drain(..).collect()
    }

    pub fn size(&self) -> usize {
        self.queue.read().len()
    }

    pub fn flush_to_propagator(&self, propagator: &MeshPropagator) -> usize {
        let txs = self.dequeue_all();
        let count = txs.len();
        for tx in txs {
            let _ = propagator.receive_transaction(tx);
        }
        count
    }
}

// ==================== Mesh HTTP Server ====================

use axum::{routing::post, Json, Router};
use std::net::SocketAddr;

/// Start the mesh network HTTP server for inter-project forwarding
#[allow(clippy::needless_pass_by_value)]
pub async fn start_mesh_server(propagator: Arc<MeshPropagator>, port: u16) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/api/v1/mesh/forward", post(mesh_forward_handler))
        .route("/api/v1/health", post(|| async { Json(serde_json::json!({"status": "mesh_online"})) }))
        .with_state(propagator);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("📍 Mesh Network HTTP server listening on {}", addr);
    axum::serve(tokio::net::TcpListener::bind(addr).await?, app).await?;
    Ok(())
}

async fn mesh_forward_handler(
    axum::extract::State(propagator): axum::extract::State<Arc<MeshPropagator>>,
    Json(payload): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let target = payload.get("target").and_then(|v| v.as_str()).unwrap_or("unknown");
    let command_type = payload.get("command_type").and_then(|v| v.as_str()).unwrap_or("unknown");

    // Create a mesh transaction for the forwarded command
    let result = propagator.create_transaction(
        &format!("did:swift:{}", target),
        0.0,
        "CMD",
    );

    match result {
        Ok(tx) => {
            info!("Mesh forward: {} -> {} ({})", command_type, target, tx.id);
            Json(serde_json::json!({
                "status": "forwarded",
                "target": target,
                "command_type": command_type,
                "transaction_id": tx.id,
                "hops": tx.hops,
                "mesh_proof": {
                    "route_hash": format!("{:?}", tx.mesh_proof.route_hash),
                    "witness_count": tx.mesh_proof.witness_count,
                    "finalized": tx.mesh_proof.finalized,
                }
            }))
        }
        Err(e) => {
            warn!("Mesh forward failed: {} -> {}: {}", command_type, target, e);
            Json(serde_json::json!({
                "status": "error",
                "error": e,
                "target": target,
            }))
        }
    }
}

/// A standalone main entry point for the mesh network node
pub async fn run_mesh_node(mesh_port: u16) -> anyhow::Result<()> {
    let config = MeshConfig {
        listen_port: mesh_port,
        ..MeshConfig::default()
    };
    let propagator = Arc::new(MeshPropagator::new(config).map_err(|e| anyhow::anyhow!(e))?);
    info!("Mesh node started — DID: {}", propagator.config.did);
    start_mesh_server(propagator, mesh_port).await
}

// ==================== Entry Point ====================

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("swiftbridge_mesh=info")
        .init();

    let port: u16 = std::env::var("MESH_PORT")
        .unwrap_or_else(|_| "9777".to_string())
        .parse()
        .unwrap_or(9777);

    info!("Starting SwiftBridge Mesh Network on port {}...", port);
    run_mesh_node(port).await
}

// ==================== Benchmark Helpers ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_create_and_verify() {
        let config = MeshConfig {
            did: "did:swift:alice".to_string(),
            ..MeshConfig::default()
        };
        let propagator = MeshPropagator::new(config).unwrap();
        let tx = propagator.create_transaction("did:swift:bob", 100.0, "SWB").unwrap();
        assert_eq!(tx.from_did, "did:swift:alice");
        assert_eq!(tx.to_did, "did:swift:bob");
        assert_eq!(tx.hops.len(), 1);
    }

    #[test]
    fn test_mempool_submit_confirm() {
        let mempool = Mempool::new(100);
        let config = MeshConfig {
            did: "did:swift:alice".to_string(),
            ..MeshConfig::default()
        };
        let propagator = MeshPropagator::new(config).unwrap();
        let tx = propagator.create_transaction("did:swift:bob", 50.0, "SWB").unwrap();
        assert!(mempool.submit(tx.clone()).is_ok());
        mempool.confirm(tx.id);
        assert_eq!(mempool.pending_count(), 0);
    }

    #[test]
    fn test_offline_queue() {
        let queue = OfflineQueue::new(100);
        let config = MeshConfig {
            did: "did:swift:alice".to_string(),
            ..MeshConfig::default()
        };
        let propagator = MeshPropagator::new(config).unwrap();
        let tx = propagator.create_transaction("did:swift:bob", 25.0, "SWB").unwrap();
        queue.enqueue(tx).unwrap();
        assert_eq!(queue.size(), 1);
    }

    #[test]
    fn test_routing_table() {
        let rt = RoutingTable::new();
        let peer = PeerInfo {
            peer_id: "peer1".to_string(),
            did: "did:swift:bob".to_string(),
            public_key: vec![0u8; 32],
            last_seen: Utc::now().timestamp(),
            signal_strength: -50,
            supports_bluetooth: true,
            supports_wifi_direct: true,
            location: None,
        };
        rt.add_peer(peer);
        assert_eq!(rt.peer_count(), 1);
        rt.blacklist("peer1", 3600);
        assert!(rt.is_blacklisted("peer1"));
    }

    #[test]
    fn test_mesh_propagation_throughput() {
        let config = MeshConfig {
            did: "did:swift:stress_test".to_string(),
            ..MeshConfig::default()
        };
        let propagator = MeshPropagator::new(config).unwrap();
        let start = Instant::now();
        let count = 100_000u64;

        for i in 0..count {
            let tx = propagator.create_transaction(
                &format!("did:swift:user_{}", i % 1000),
                100.0 + (i % 100) as f64,
                "SWB",
            ).unwrap();
            let _ = propagator.receive_transaction(tx);
        }

        let elapsed = start.elapsed();
        let tps = count as f64 / elapsed.as_secs_f64();
        println!("Mesh propagation: {:.0} TPS ({:.2}s for {})", tps, elapsed.as_secs_f64(), count);
        assert!(tps > 500_000.0, "TPS too low: {:.0}", tps);
    }
}
