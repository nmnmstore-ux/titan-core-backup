use crate::io::{self, TimestampSource, Transport};
use blake2::Digest;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{RwLock, mpsc};
use tokio::time::sleep;

const GOSSIP_INTERVAL_MS: u64 = 50;
const MAX_PARENTS: usize = 3;
const FINALIZATION_DEPTH: u64 = 3;
const CONSENSUS_PORT: u16 = 4002;
const MAX_MEMPOOL: usize = 10_000;
const MEMPOOL_FLUSH_INTERVAL_MS: u64 = 25;
const _HANDSHAKE_TIMEOUT_MS: u64 = 2000;
const _MAX_RECONNECT_BACKOFF: u64 = 30_000;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConsensusOp {
    PlaceOrder(crate::types::Order),
    CancelOrder(uuid::Uuid),
    SettleDOT(crate::types::DOTTransfer),
    SnapshotSync(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DAGVertex {
    pub hash: VertexHash,
    pub parents: Vec<VertexHash>,
    pub operation: ConsensusOp,
    pub timestamp: i64,
    pub node_id: String,
    pub signature: Vec<u8>,
}

impl DAGVertex {
    pub fn compute_hash(&self) -> VertexHash {
        let bytes = bincode::serialize(&(
            &self.parents, &self.operation, self.timestamp, &self.node_id,
        ))
        .unwrap_or_default();
        let mut hasher = blake2::Blake2b512::new();
        hasher.update(&bytes);
        let mut out = [0u8; 64];
        out.copy_from_slice(&hasher.finalize());
        VertexHash(out)
    }

    pub fn new(operation: ConsensusOp, parents: Vec<VertexHash>, node_id: &str, timestamp_ns: i64) -> Self {
        let mut v = Self {
            hash: VertexHash([0; 64]),
            parents,
            operation,
            timestamp: timestamp_ns,
            node_id: node_id.to_string(),
            signature: Vec::new(),
        };
        v.hash = v.compute_hash();
        v
    }

    pub fn new_now(operation: ConsensusOp, parents: Vec<VertexHash>, node_id: &str) -> Self {
        Self::new(operation, parents, node_id, chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0))
    }

    pub fn sign(&mut self, signing_key: &ed25519_dalek::SigningKey) {
        use ed25519_dalek::Signer;
        let bytes = bincode::serialize(&(
            &self.parents, &self.operation, self.timestamp, &self.node_id,
        )).unwrap_or_default();
        let sig = signing_key.sign(&bytes);
        self.signature = sig.to_bytes().to_vec();
    }

    pub fn verify(&self, verifying_key: &ed25519_dalek::VerifyingKey) -> bool {
        if self.signature.len() != 64 {
            return false;
        }
        use ed25519_dalek::{Verifier, Signature};
        let bytes = bincode::serialize(&(
            &self.parents, &self.operation, self.timestamp, &self.node_id,
        )).unwrap_or_default();
        let sig = match Signature::from_slice(&self.signature) {
            Ok(s) => s,
            Err(_) => return false,
        };
        verifying_key.verify(&bytes, &sig).is_ok()
    }
}

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

    pub async fn submit(&self, op: ConsensusOp) {
        let selected_parents = self.select_tips().await;
        let ts = self.timestamp_source.now_ns();
        let mut vertex = DAGVertex::new(op, selected_parents, &self.node_id, ts);
        vertex.sign(&self.signing_key);
        let hash = vertex.hash.clone();
        let arc = Arc::new(vertex);

        self.dag.write().await.insert(hash.clone(), arc);
        self.tips.write().await.push(hash.clone());
        self.prune_tips().await;
        self.broadcast_hash(&hash).await;
    }

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

    async fn send_message(&self, addr: &str, msg: &PeerMessage) -> Result<(), String> {
        let data = bincode::serialize(msg).map_err(|e| format!("serialize: {}", e))?;
        self.transport.send(addr, &data).await
    }

    pub async fn listen(self: Arc<Self>) {
        let addr = format!("0.0.0.0:{}", CONSENSUS_PORT);

        // Start transport listener — handles accept + read in background
        let gossip_tx = self.gossip_tx.clone();
        let on_msg = Arc::new(move |data: Vec<u8>, _peer: String| {
            if let Ok(msg) = bincode::deserialize::<PeerMessage>(&data) {
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

    pub async fn peer_handshake_loop(&self) -> Result<(), String> {
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

    pub async fn submit_with_verification(&self, vertex: DAGVertex) {
        let hash = vertex.hash.clone();
        if !vertex.verify(&self.verifying_key) {
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


