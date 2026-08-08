//! DAG-based consensus protocol for decentralized settlement.
//!
//! Implements a Directed Acyclic Graph (DAG) consensus where each vertex
//! references 1–`MAX_PARENTS` previous vertices, forming a tangle structure.
//! Unlike a blockchain, multiple tips can coexist and are reconciled through
//! tip selection and finalization rules.
//!
//! # DAG Structure
//!
//! ```text
//!     ┌─ A ─┐
//!     │     │
//!   genesis │
//!     │     │
//!     └─ B ─┤
//!           │
//!     ┌─ C ─┘
//!     │
//!     └─ D (tip)
//! ```
//!
//! - Each `DAGVertex` has a Blake2b-512 hash computed from its content.
//! - Parents are selected from current tips via randomized conflict-aware selection.
//! - `MAX_PARENTS = 3` limits the branching factor.
//!
//! # Vertex Lifecycle
//!
//! ```text
//! 1. ENQUEUED  → Op enters mempool via `enqueue_op()`.
//! 2. SUBMITTED  → `submit()` selects parents, signs, broadcasts to peers.
//! 3. RECEIVED   → Remote vertex passes signature verification.
//! 4. FINALIZED  → Vertex is FINALIZATION_DEPTH (3) deep from a single tip.
//! ```
//!
//! # Tip Selection
//!
//! `select_tips()` picks up to `MAX_PARENTS` current tips:
//! 1. If candidates ≤ MAX_PARENTS, use all.
//! 2. Otherwise, randomly shuffle and filter out conflicting tips
//!    (e.g., two tips that cancel the same order).
//! 3. Conflict check prevents double-execution of conflicting operations.
//!
//! # Verification Protocol
//!
//! Incoming vertices are verified before insertion:
//! 1. `creator_key` must be exactly 32 bytes (Ed25519 public key).
//! 2. Ed25519 signature must verify over `(parents, operation, timestamp, node_id, creator_key)`.
//! 3. Rejected vertices are logged and dropped.
//!
//! # Gossip Protocol
//!
//! - **Interval**: Every 50ms, broadcast all unsent tips to all peers.
//! - **Transport**: TCP on port 4002 (configurable via `CONSENSUS_PORT`).
//! - **Handshake**: Hello/Ack exchange with node_id + public_key.
//! - **Mempool**: Operations buffered up to `MAX_MEMPOOL` (10k), flushed every 25ms.

use crate::io::{self, TimestampSource, Transport};
use blake2::Digest;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::{RwLock, mpsc};
use tokio::time::sleep;

#[derive(Debug, Error)]
pub enum ConsensusError {
    #[error("consensus serialization error: {0}")]
    Serialize(String),
    #[error("consensus transport error: {0}")]
    Transport(String),
    #[error("consensus vertex rejected: {0}")]
    VertexRejected(String),
}

impl From<ConsensusError> for String {
    fn from(e: ConsensusError) -> String {
        e.to_string()
    }
}
use tracing::instrument;

/// Interval in milliseconds between gossip rounds (broadcast unsent tips).
const GOSSIP_INTERVAL_MS: u64 = 50;
/// Maximum number of parents a new vertex can reference.
const MAX_PARENTS: usize = 3;
/// Number of levels deep a vertex must be from a single tip to be finalized.
const FINALIZATION_DEPTH: u64 = 3;
/// TCP port for consensus peer communication.
const CONSENSUS_PORT: u16 = 4002;
/// Maximum mempool capacity before new ops are dropped.
const MAX_MEMPOOL: usize = 10_000;
/// Interval in milliseconds for flushing the mempool into submitted vertices.
const MEMPOOL_FLUSH_INTERVAL_MS: u64 = 25;
const _HANDSHAKE_TIMEOUT_MS: u64 = 2000;
const _MAX_RECONNECT_BACKOFF: u64 = 30_000;

/// Blake2b-512 hash of a DAG vertex. Used as the vertex identifier.
///
/// Computed deterministically from `(parents, operation, timestamp, node_id, creator_key)`.
/// Serialized as a 64-byte hex string for wire transport.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct VertexHash(pub [u8; 64]);

impl Serialize for VertexHash {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        hex::encode(self.0).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for VertexHash {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        let bytes = hex::decode(&s).map_err(serde::de::Error::custom)?;
        if bytes.len() != 64 {
            return Err(serde::de::Error::custom("expected 64 bytes"));
        }
        let mut arr = [0u8; 64];
        arr.copy_from_slice(&bytes);
        Ok(VertexHash(arr))
    }
}

/// Consensus operation — the payload carried by a DAG vertex.
///
/// Each variant maps to a specific engine mutation that must be agreed
/// upon by all nodes before execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConsensusOp {
    PlaceOrder(crate::types::Order),
    CancelOrder(uuid::Uuid),
    SettleDOT(crate::types::DOTTransfer),
    SnapshotSync(String),
}

/// A vertex in the DAG consensus graph.
///
/// Each vertex contains:
/// - `hash`: Blake2b-512 content hash (computed, not stored in serialization).
/// - `parents`: 1–3 parent vertex hashes (the vertex's "references").
/// - `operation`: The consensus operation (order, cancel, DOT, snapshot).
/// - `timestamp`: Nanosecond-precision creation time.
/// - `node_id`: Identifying string of the creating node.
/// - `signature`: Ed25519 signature over the vertex content.
/// - `creator_key`: Ed25519 public key of the creator (32 bytes).
///
/// # Hash Computation
/// `hash = Blake2b512(bincode(parents, operation, timestamp, node_id, creator_key))`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DAGVertex {
    pub hash: VertexHash,
    pub parents: Vec<VertexHash>,
    pub operation: ConsensusOp,
    pub timestamp: i64,
    pub node_id: String,
    pub signature: Vec<u8>,
    pub creator_key: Vec<u8>,
}

impl DAGVertex {
    pub fn compute_hash(&self) -> VertexHash {
        let bytes = crate::types::bincode_serialize_direct(&(
            &self.parents, &self.operation, self.timestamp, &self.node_id, &self.creator_key,
        ))
        .unwrap_or_default();
        let mut hasher = blake2::Blake2b512::new();
        hasher.update(&bytes);
        let mut out = [0u8; 64];
        out.copy_from_slice(&hasher.finalize());
        VertexHash(out)
    }

    pub fn new(operation: ConsensusOp, parents: Vec<VertexHash>, node_id: &str, timestamp_ns: i64, creator_key: Vec<u8>) -> Self {
        let mut v = Self {
            hash: VertexHash([0; 64]),
            parents,
            operation,
            timestamp: timestamp_ns,
            node_id: node_id.to_string(),
            signature: Vec::new(),
            creator_key,
        };
        v.hash = v.compute_hash();
        v
    }

    pub fn new_now(operation: ConsensusOp, parents: Vec<VertexHash>, node_id: &str, creator_key: Vec<u8>) -> Self {
        Self::new(operation, parents, node_id, chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0), creator_key)
    }

    pub fn sign(&mut self, signing_key: &ed25519_dalek::SigningKey) {
        use ed25519_dalek::Signer;
        let bytes = crate::types::bincode_serialize_direct(&(
            &self.parents, &self.operation, self.timestamp, &self.node_id, &self.creator_key,
        )).unwrap_or_default();
        let sig = signing_key.sign(&bytes);
        self.signature = sig.to_bytes().to_vec();
    }

    pub fn verify(&self, verifying_key: &ed25519_dalek::VerifyingKey) -> bool {
        if self.signature.len() != 64 {
            return false;
        }
        use ed25519_dalek::{Verifier, Signature};
        let bytes = crate::types::bincode_serialize_direct(&(
            &self.parents, &self.operation, self.timestamp, &self.node_id, &self.creator_key,
        )).unwrap_or_default();
        let sig = match Signature::from_slice(&self.signature) {
            Ok(s) => s,
            Err(_) => return false,
        };
        verifying_key.verify(&bytes, &sig).is_ok()
    }
}

/// Peer-to-peer message type for the gossip protocol.
///
/// All messages are bincode-serialized and sent over TCP.
#[derive(Debug, Clone, Serialize, Deserialize)]
enum PeerMessage {
    Hello { node_id: String, public_key: Vec<u8> },
    Ack { node_id: String },
    Vertex { vertex: DAGVertex },
    BatchVertices { vertices: Vec<DAGVertex> },
    SnapshotRequest { last_hash: VertexHash },
    SnapshotResponse { vertices: Vec<DAGVertex> },
    Ping(i64),
    Pong(i64),
}

/// DAG-based consensus engine for multi-node agreement.
///
/// # State
/// - `dag`: Full vertex graph (hash → vertex).
/// - `tips`: Current unfinalized tips (vertices with no children yet).
/// - `finalized`: Vertices confirmed by depth threshold.
/// - `mempool`: Operations waiting to be submitted as vertices.
/// - `sent_hashes`: Tracks which tips have been gossiped (avoid re-sending).
///
/// # Concurrency
/// All mutable state is behind `tokio::sync::RwLock` for async compatibility.
/// The gossip loop and listener run as separate tokio tasks.
pub struct DAGConsensus {
    node_id: String,
    dag: RwLock<HashMap<VertexHash, Arc<DAGVertex>>>,
    tips: RwLock<Vec<VertexHash>>,
    finalized: RwLock<Vec<VertexHash>>,
    peers: Vec<String>,
    pending_ops: RwLock<VecDeque<ConsensusOp>>,
    mempool: RwLock<VecDeque<ConsensusOp>>,
    signing_key: ed25519_dalek::SigningKey,
    verifying_key: ed25519_dalek::VerifyingKey,
    #[allow(dead_code)]
    verifying_key_hex: String,
    sent_hashes: RwLock<HashSet<VertexHash>>,
    healthy: RwLock<bool>,
    #[allow(dead_code)]
    last_gossip: RwLock<HashMap<String, std::time::Instant>>,
    gossip_tx: mpsc::UnboundedSender<PeerMessage>,
    gossip_rx: RwLock<Option<mpsc::UnboundedReceiver<PeerMessage>>>,
    transport: Arc<dyn Transport + 'static>,
    timestamp_source: Arc<dyn TimestampSource + 'static>,
}

impl DAGConsensus {
    pub fn new(node_id: &str, peers: Vec<String>, key_bytes: &[u8; 32]) -> Self {
        Self::with_transport(
            node_id, peers, key_bytes,
            Arc::new(io::TCP_TRANSPORT),
            Arc::new(io::TIMESTAMP_SOURCE),
        )
    }

    pub fn with_transport(
        node_id: &str,
        peers: Vec<String>,
        key_bytes: &[u8; 32],
        transport: Arc<dyn Transport + 'static>,
        timestamp_source: Arc<dyn TimestampSource + 'static>,
    ) -> Self {
        let signing_key = ed25519_dalek::SigningKey::from_bytes(key_bytes);
        let verifying_key = signing_key.verifying_key();
        let verifying_key_hex = hex::encode(verifying_key.as_bytes());
        let (tx, rx): (mpsc::UnboundedSender<PeerMessage>, mpsc::UnboundedReceiver<PeerMessage>) = mpsc::unbounded_channel();
        Self {
            node_id: node_id.to_string(),
            dag: RwLock::new(HashMap::new()),
            tips: RwLock::new(Vec::new()),
            finalized: RwLock::new(Vec::new()),
            peers,
            pending_ops: RwLock::new(VecDeque::new()),
            mempool: RwLock::new(VecDeque::with_capacity(MAX_MEMPOOL)),
            signing_key,
            verifying_key,
            verifying_key_hex,
            sent_hashes: RwLock::new(HashSet::new()),
            healthy: RwLock::new(true),
            last_gossip: RwLock::new(HashMap::new()),
            gossip_tx: tx,
            gossip_rx: RwLock::new(Some(rx)),
            transport,
            timestamp_source,
        }
    }

    /// Submit an operation directly to the DAG.
    ///
    /// Selects parents from current tips, creates and signs a vertex,
    /// inserts it into the DAG, updates tips, and broadcasts to peers.
    #[instrument(skip(self), fields(node_id = %self.node_id))]
    pub async fn submit(&self, op: ConsensusOp) {
        let selected_parents = self.select_tips().await;
        let ts = self.timestamp_source.now_ns();
        let mut vertex = DAGVertex::new(op, selected_parents, &self.node_id, ts, self.verifying_key.as_bytes().to_vec());
        vertex.sign(&self.signing_key);
        let hash = vertex.hash.clone();
        let arc = Arc::new(vertex);

        self.dag.write().await.insert(hash.clone(), arc);
        self.tips.write().await.push(hash.clone());
        self.prune_tips().await;
        self.broadcast_hash(&hash).await;
        tracing::info!(vertex_hash = ?hash, "consensus vertex submitted");
    }

    /// Enqueue an operation into the mempool for deferred submission.
    ///
    /// Mempool is flushed periodically by `mempool_loop()` to batch
    /// multiple operations into fewer gossip rounds.
    pub async fn enqueue_op(&self, op: ConsensusOp) {
        let mut mempool = self.mempool.write().await;
        if mempool.len() < MAX_MEMPOOL {
            mempool.push_back(op);
        }
    }

    pub async fn mempool_depth(&self) -> usize {
        self.mempool.read().await.len()
    }

    pub async fn flush_mempool(&self) {
        let batch: Vec<ConsensusOp> = {
            let mut mempool = self.mempool.write().await;
            mempool.drain(..).collect()
        };
        for op in batch {
            self.submit(op).await;
        }
    }

    pub async fn mempool_loop(&self) {
        loop {
            sleep(Duration::from_millis(MEMPOOL_FLUSH_INTERVAL_MS)).await;
            let depth = self.mempool.read().await.len();
            if depth > 0 {
                self.flush_mempool().await;
            }
        }
    }

    async fn broadcast_hash(&self, hash: &VertexHash) {
        let dag = self.dag.read().await;
        let vertex = match dag.get(hash) {
            Some(v) => v.clone(),
            None => return,
        };
        let msg = PeerMessage::Vertex { vertex: (*vertex).clone() };
        for peer_addr in &self.peers {
            let _ = self.send_message(peer_addr, &msg).await;
        }
    }

    async fn send_message(&self, addr: &str, msg: &PeerMessage) -> Result<(), ConsensusError> {
        let data = crate::types::bincode_serialize_direct(msg).map_err(|e| ConsensusError::Serialize(e.to_string()))?;
        self.transport.send(addr, &data).await.map_err(|e| ConsensusError::Transport(e))
    }

    pub async fn listen(self: Arc<Self>) {
        let addr = format!("0.0.0.0:{}", CONSENSUS_PORT);

        // Start transport listener — handles accept + read in background
        let gossip_tx = self.gossip_tx.clone();
        let on_msg = Arc::new(move |data: Vec<u8>, _peer: String| {
            if let Ok(msg) = crate::types::bincode_deserialize_direct::<PeerMessage>(&data) {
                let _ = gossip_tx.send(msg);
            }
        });
        let transport = self.transport.clone();
        transport.listen(&addr, on_msg);

        // Internal gossip channel loop — processes incoming from transport + local
        let rx = self.gossip_rx.write().await.take();
        let consensus = self.clone();
        tokio::spawn(async move {
            let mut rx = match rx {
                Some(r) => r,
                None => return,
            };
            loop {
                match rx.recv().await {
                    Some(PeerMessage::Vertex { vertex }) => {
                        consensus.submit_with_verification(vertex).await;
                    }
                    Some(PeerMessage::BatchVertices { vertices }) => {
                        for v in vertices {
                            consensus.submit_with_verification(v).await;
                        }
                    }
                    Some(PeerMessage::Hello { node_id, public_key: _ }) => {
                        tracing::info!(peer = %node_id, "consensus: handshake received");
                        let ack = PeerMessage::Ack { node_id: node_id.clone() };
                        let _ = consensus.gossip_tx.send(ack);
                    }
                    Some(PeerMessage::Ack { node_id }) => {
                        tracing::info!(peer = %node_id, "consensus: handshake acknowledged");
                    }
                    Some(PeerMessage::Ping(ts)) => {
                        let pong = PeerMessage::Pong(ts);
                        let _ = consensus.gossip_tx.send(pong);
                    }
                    _ => {
                        tracing::trace!("consensus: received internal message");
                    }
                }
            }
        });
    }

    pub async fn gossip_loop(&self) {
        loop {
            sleep(Duration::from_millis(GOSSIP_INTERVAL_MS)).await;

            let tips = self.tips.read().await.clone();
            let mut sent = self.sent_hashes.write().await;

            for tip in &tips {
                if sent.contains(tip) {
                    continue;
                }
                sent.insert(tip.clone());
                let dag = self.dag.read().await;
                let vertex = match dag.get(tip) {
                    Some(v) => (*v).clone(),
                    None => continue,
                };
                drop(dag);

                let msg = PeerMessage::Vertex { vertex: (*vertex).clone() };
                for peer_addr in &self.peers {
                    let _ = self.send_message(peer_addr, &msg).await;
                }
            }
        }
    }

    pub async fn peer_handshake_loop(&self) -> Result<(), ConsensusError> {
        for peer_addr in &self.peers {
            let hello = PeerMessage::Hello {
                node_id: self.node_id.clone(),
                public_key: self.verifying_key.as_bytes().to_vec(),
            };
            match self.send_message(peer_addr, &hello).await {
                Ok(_) => {
                    tracing::info!(peer = %peer_addr, "consensus: handshake sent");
                }
                Err(e) => {
                    tracing::warn!(peer = %peer_addr, error = %e, "consensus: handshake failed");
                }
            }
        }
        Ok(())
    }

    /// Select up to `MAX_PARENTS` tips for a new vertex.
    ///
    /// If candidates ≤ MAX_PARENTS, all are used. Otherwise, tips are
    /// randomly shuffled and filtered for conflicts (e.g., duplicate
    /// cancel operations on the same order).
    pub async fn select_tips(&self) -> Vec<VertexHash> {
        let tips = self.tips.read().await;
        if tips.is_empty() { return Vec::new(); }

        let candidates: Vec<&VertexHash> = tips.iter().collect();
        if candidates.len() <= MAX_PARENTS {
            return candidates.into_iter().cloned().collect();
        }

        let mut selected = Vec::new();
        let mut used = HashSet::new();
        let dag = self.dag.read().await;

        for _ in 0..MAX_PARENTS {
            let shuffled = {
                let mut c = candidates.clone();
                use rand::seq::SliceRandom;
                let mut rng = rand::thread_rng();
                c.shuffle(&mut rng);
                c
            };
            for tip in shuffled {
                if used.contains(tip) { continue; }
                let conflicts = self.check_conflicts(tip, &used, &dag).await;
                if !conflicts {
                    selected.push(tip.clone());
                    used.insert(tip.clone());
                    break;
                }
            }
        }
        selected
    }

    async fn check_conflicts(&self, tip: &VertexHash, used: &HashSet<VertexHash>, dag: &HashMap<VertexHash, Arc<DAGVertex>>) -> bool {
        if let Some(vertex) = dag.get(tip) {
            match &vertex.operation {
                ConsensusOp::CancelOrder(id) => {
                    for used_hash in used {
                        if let Some(used_v) = dag.get(used_hash) {
                            match &used_v.operation {
                                ConsensusOp::CancelOrder(uid) | ConsensusOp::PlaceOrder(crate::types::Order { id: uid, .. }) => {
                                    if uid == id { return true; }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        false
    }

    /// Prune tips that are no longer tips (have been referenced by a child).
    ///
    /// Also triggers finalization check: if only one tip remains and it is
    /// `FINALIZATION_DEPTH` deep, it is moved to the finalized list.
    async fn prune_tips(&self) {
        let dag = self.dag.read().await;
        let mut tips = self.tips.write().await;
        let mut to_remove = Vec::new();

        for (i, tip) in tips.iter().enumerate() {
            if let Some(vertex) = dag.get(tip) {
                for parent in &vertex.parents {
                    if tips.contains(parent) {
                        to_remove.push(i);
                        break;
                    }
                }
            }
        }

        for i in to_remove.into_iter().rev() {
            tips.swap_remove(i);
        }

        self.finalize_vertices(&dag, &tips).await;
    }

    /// Finalize a vertex if it is deep enough from the single remaining tip.
    ///
    /// Walks parent pointers from the tip. If `FINALIZATION_DEPTH` (3) ancestors
    /// are reached, the tip is considered finalized and appended to `self.finalized`.
    #[instrument(skip(self, dag, tips), fields(tip_count = tips.len()))]
    async fn finalize_vertices(&self, dag: &HashMap<VertexHash, Arc<DAGVertex>>, tips: &[VertexHash]) {
        if tips.len() > 1 { return; }
        if let Some(tip) = tips.first() {
            if let Some(_vertex) = dag.get(tip) {
                let mut depth = 0u64;
                let mut current = tip.clone();
                loop {
                    if let Some(v) = dag.get(&current) {
                        if v.parents.is_empty() { break; }
                        current = v.parents[0].clone();
                        depth += 1;
                        if depth >= FINALIZATION_DEPTH {
                            self.finalized.write().await.push(tip.clone());
                            tracing::info!(vertex_hash = ?tip, depth, "consensus vertex finalized");
                            break;
                        }
                    } else { break; }
                }
            }
        }
    }

    pub async fn num_vertices(&self) -> usize {
        self.dag.read().await.len()
    }

    pub async fn num_finalized(&self) -> usize {
        self.finalized.read().await.len()
    }

    pub async fn num_tips(&self) -> usize {
        self.tips.read().await.len()
    }

    pub async fn is_healthy(&self) -> bool {
        *self.healthy.read().await
    }

    /// Verify and insert a vertex received from a peer.
    ///
    /// Rejects vertices with invalid creator_key length, invalid Ed25519
    /// signatures, or malformed data. Valid vertices are inserted into
    /// the DAG, added as a tip, and trigger tip pruning.
    pub async fn submit_with_verification(&self, vertex: DAGVertex) {
        let hash = vertex.hash.clone();
        if vertex.creator_key.len() != 32 {
            tracing::warn!(hash = ?hash, "consensus: vertex rejected — invalid creator_key length");
            return;
        }
        let creator_vk = match ed25519_dalek::VerifyingKey::from_bytes(
            vertex.creator_key.as_slice().try_into().unwrap_or(&[0u8; 32]),
        ) {
            Ok(vk) => vk,
            Err(_) => {
                tracing::warn!(hash = ?hash, "consensus: vertex rejected — invalid creator_key");
                return;
            }
        };
        if !vertex.verify(&creator_vk) {
            tracing::warn!(hash = ?hash, "consensus: vertex signature rejected");
            return;
        }
        self.dag.write().await.insert(hash.clone(), Arc::new(vertex));
        self.tips.write().await.push(hash.clone());
        self.prune_tips().await;
    }

    pub async fn pending_ops_count(&self) -> usize {
        self.pending_ops.read().await.len()
    }
}


