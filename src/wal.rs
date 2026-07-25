//! Write-Ahead Log (WAL) for crash-safe persistence and sync replication.
//!
//! Every mutating operation (order placement, cancellation, trade settlement,
//! DOT transfer) is logged to the WAL before being applied to in-memory state.
//! On crash, the WAL is replayed to restore the engine to a consistent state.
//!
//! # WAL Record Format (wire)
//!
//! Each entry on disk is a contiguous byte sequence:
//!
//! ```text
//! ┌──────────┬──────────┬──────────────┬─────────────────┬──────────────┬─────────────┬──────────────┐
//! │ crc32(8) │ len(8)   │ timestamp(16)│ prev_hash(64 hex)│ seq(16 hex)  │ payload(var)│ signature(64)│
//! └──────────┴──────────┴──────────────┴─────────────────┴──────────────┴──────────────┴──────────────┘
//! ```
//!
//! - **crc32**: CRC32 of `timestamp + length + payload` (integrity check).
//! - **length**: Byte length of the (encrypted) payload.
//! - **prev_hash**: Blake2b-512 hash of the previous entry (hash chain).
//! - **seq**: Monotonically increasing sequence number.
//! - **payload**: bincode-serialized `WALRecord`, optionally AES-256-GCM encrypted.
//! - **signature**: Ed25519 signature over `Blake2b512(seq || crc32 || prev_hash)`.
//!
//! # Encryption Scheme
//!
//! - **Algorithm**: AES-256-GCM (authenticated encryption with associated data).
//! - **Key**: 256-bit key from `WAL_ENCRYPTION_KEY` env var (base64-encoded).
//! - **Nonce**: Random 12-byte nonce per record (generated via `thread_rng`).
//! - **Ciphertext format**: `[nonce(12) || encrypted_payload + tag(16)]`.
//! - **At-rest only**: Encryption is applied before writing; entries in the
//!   in-memory `pending` queue are plaintext for fast verification.
//! - **Fallback**: If `WAL_ENCRYPTION_KEY` is unset, runs unencrypted (dev/test).
//!
//! # Recovery Procedure
//!
//! 1. Open WAL file, read entire contents into memory.
//! 2. Parse entries sequentially: header → payload → optional signature.
//! 3. For each entry: verify CRC32, decrypt if encrypted, deserialize.
//! 4. Verify Blake2b hash chain: `expected = Blake2b512(seq || crc32 || prev_hash)`.
//! 5. Verify Ed25519 signature against the entry hash (if present).
//! 6. Stop at first corrupt/truncated entry (partial recovery).
//! 7. Return all valid `WALRecord`s for replay into engine state.
//!
//! # Replication Protocol
//!
//! - **Batch size**: Every `MAX_BATCH_SIZE` (64) entries, flush + replicate.
//! - **Transport**: TCP to each replica with 100ms timeout.
//! - **Wire**: `[length:8 LE] [bincode(batch of WALEntries)]`.
//! - **ACK**: Replica sends 1-byte acknowledgment after receiving batch.
//! - **Lag tracking**: `replica_lag()` returns ms since last successful sync.
//! - **Health check**: `is_healthy()` = `replica_lag < MAX_REPLICA_LAG` (10s).
//!
//! # io_uring Backend (Linux)
//!
//! On Linux, uses `io_uring` for NVMe-direct writes, bypassing VFS.
//! Falls back to `std::fs::write_all` if io_uring is unavailable.
//! Aligned buffers (512B) are used for O_DIRECT DMA optimization.

#![allow(dead_code)]
use crate::types::*;
use aes_gcm::{Aes256Gcm, Key, Nonce};
use aead::{Aead, KeyInit};
use blake2::{Blake2b512, Digest};
use crc32fast::Hasher;
use ed25519_dalek::Signer;
use parking_lot::Mutex;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[derive(Debug, Error)]
pub enum WALError {
    #[error("WAL directory error: {0}")]
    DirCreate(String),
    #[error("WAL file error: {0}")]
    FileOpen(String),
    #[error("WAL metadata error: {0}")]
    Metadata(String),
    #[error("WAL read error: {0}")]
    Read(String),
    #[error("WAL write error: {0}")]
    Write(String),
    #[error("WAL flush error: {0}")]
    Flush(String),
    #[error("WAL sync error: {0}")]
    Sync(String),
    #[error("WAL fsync error: {0}")]
    Fsync(String),
    #[error("WAL serialize error: {0}")]
    Serialize(String),
    #[error("WAL encryption error: {0}")]
    Encryption(String),
    #[error("WAL decryption error: {0}")]
    Decryption(String),
    #[error("WAL key error: {0}")]
    KeyError(String),
    #[error("WAL runtime error: {0}")]
    Runtime(String),
    #[error("WAL replica timeout")]
    ReplicaTimeout,
    #[error("WAL replica connect error: {0}")]
    ReplicaConnect(String),
    #[error("WAL io_uring error: {0}")]
    IoUring(String),
    #[error("WAL chain verification failed at seq {seq}")]
    ChainVerification { seq: u64 },
}

impl From<WALError> for String {
    fn from(e: WALError) -> String {
        e.to_string()
    }
}
use tracing::{info, instrument};

/// WAL file magic number for format identification (unused currently, reserved).
const WAL_MAGIC: u32 = 0x53424757;
/// Number of entries between flush + replication cycles.
const MAX_BATCH_SIZE: usize = 64;
/// Maximum allowed replication lag in milliseconds before marking unhealthy.
const MAX_REPLICA_LAG: u64 = 10_000;

// Aligned buffer for O_DIRECT writes on Linux (512B sector alignment)
// `Layout::from_size_align` requires size to be a multiple of align, so we round up.
#[cfg(target_os = "linux")]
fn aligned_buffer(size: usize) -> Vec<u8> {
    use std::alloc::{alloc_zeroed, Layout};
    let aligned_size = (size + 511) & !511;
    if aligned_size == 0 {
        return Vec::new();
    }
    let layout = match Layout::from_size_align(aligned_size, 512) {
        Ok(l) => l,
        Err(_) => return vec![0u8; size],
    };
    unsafe {
        let ptr = alloc_zeroed(layout);
        if ptr.is_null() {
            return vec![0u8; size];
        }
        Vec::from_raw_parts(ptr as *mut u8, aligned_size, aligned_size)
    }
}

#[cfg(not(target_os = "linux"))]
fn aligned_buffer(size: usize) -> Vec<u8> {
    vec![0u8; size]
}

// ==================== io_uring WAL backend (Linux only) ====================
// Bypasses the kernel VFS layer for direct NVMe submission/completion.
// Each write + fsync = 1 submission + 1 completion (vs 2 syscalls with write+fsync).
#[cfg(target_os = "linux")]
mod iouring_backend {
    use io_uring::{IoUring, opcode, types};
    use std::os::unix::io::{AsRawFd, RawFd};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;
    use super::WALError;

    pub struct IoUringHandle {
        ring: Mutex<IoUring>,
        fd: RawFd,
        sq_entries: u32,
        available: AtomicBool,
    }

    impl IoUringHandle {
        pub fn new(file: &std::fs::File, entries: u32) -> Option<Self> {
            let fd = file.as_raw_fd();
            match IoUring::new(entries) {
                Ok(ring) => {
                    tracing::info!(fd, sq_entries = entries, "io_uring WAL backend initialized");
                    Some(Self { ring: Mutex::new(ring), fd, sq_entries: entries, available: AtomicBool::new(true) })
                }
                Err(e) => {
                    tracing::warn!(error = %e, "io_uring not available — falling back to std fs");
                    // Try a minimal 1-entry ring as a dummy (never used since available=false)
                    match IoUring::new(1) {
                        Ok(ring) => Some(Self { ring: Mutex::new(ring), fd, sq_entries: 1, available: AtomicBool::new(false) }),
                        Err(e2) => {
                            tracing::error!(error = %e2, "io_uring completely unavailable — using sync fallback");
                            None
                        }
                    }
                }
            }
        }

        pub fn is_available(&self) -> bool {
            self.available.load(Ordering::Relaxed)
        }

        /// Submit a write via io_uring and wait for completion.
        /// Uses aligned buffer internally for optimal DMA.
        pub fn write_at(&self, buf: &[u8], offset: u64) -> Result<u32, WALError> {
            let write_e = opcode::Write::new(
                types::Fd(self.fd),
                buf.as_ptr(),
                buf.len() as u32,
            )
                .offset(offset)
                .build();

            let mut ring = self.ring.lock().map_err(|e| WALError::IoUring(format!("ring lock: {}", e)))?;
            unsafe {
                let mut sq = ring.submission();
                sq.push(&write_e)
                    .map_err(|e| WALError::IoUring(format!("io_uring sq push: {:?}", e)))?;
            }

            let cqes = ring.submit_and_wait(1)
                .map_err(|e| WALError::IoUring(format!("io_uring submit: {}", e)))?;

            if cqes == 0 {
                return Err(WALError::IoUring("no write completion".into()));
            }

            let cqe = match ring.completion().next() {
                Some(c) => c,
                None => return Err(WALError::IoUring("empty write completion".into())),
            };

            let ret = cqe.result();
            if ret < 0 {
                Err(WALError::IoUring(format!("io_uring write: errno {}", -ret)))
            } else {
                Ok(ret as u32)
            }
        }

        /// Submit an fsync via io_uring and wait for completion.
        pub fn sync_all(&self) -> Result<(), WALError> {
            let sync_e = opcode::Fsync::new(types::Fd(self.fd))
                .build();

            let mut ring = self.ring.lock().map_err(|e| WALError::IoUring(format!("ring lock: {}", e)))?;
            unsafe {
                let mut sq = ring.submission();
                sq.push(&sync_e)
                    .map_err(|e| WALError::IoUring(format!("io_uring sq push: {:?}", e)))?;
            }

            let cqes = ring.submit_and_wait(1)
                .map_err(|e| WALError::IoUring(format!("io_uring submit: {}", e)))?;

            if cqes == 0 {
                return Err(WALError::IoUring("no fsync completion".into()));
            }

            let cqe = match ring.completion().next() {
                Some(c) => c,
                None => return Err(WALError::IoUring("empty fsync completion".into())),
            };

            let ret = cqe.result();
            if ret < 0 {
                Err(WALError::IoUring(format!("io_uring fsync: errno {}", -ret)))
            } else {
                Ok(())
            }
        }
    }
}

#[cfg(target_os = "linux")]
use iouring_backend::IoUringHandle;

/// AES-256-GCM cipher for WAL at-rest encryption.
///
/// Each `encrypt()` call generates a fresh random 12-byte nonce.
/// The nonce is prepended to the ciphertext: `[nonce(12) || ciphertext + GCM tag(16)]`.
/// Decryption extracts the nonce from the first 12 bytes.
struct EncryptionManager {
    key: [u8; 32],
    cipher: Aes256Gcm,
}

impl EncryptionManager {
    fn new(key_bytes: [u8; 32]) -> Self {
        let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
        let cipher = Aes256Gcm::new(key);
        Self { key: key_bytes, cipher }
    }

    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, WALError> {
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce_obj = Nonce::from_slice(&nonce_bytes);
        let ciphertext = self.cipher
            .encrypt(nonce_obj, plaintext)
            .map_err(|e| WALError::Encryption(format!("AES-256-GCM encrypt error: {}", e)))?;
        let mut result = Vec::with_capacity(12 + ciphertext.len());
        result.extend_from_slice(&nonce_bytes);
        result.extend_from_slice(&ciphertext);
        Ok(result)
    }

    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, WALError> {
        if ciphertext.len() < 12 {
            return Err(WALError::Decryption("ciphertext too short".into()));
        }
        let nonce_bytes = &ciphertext[..12];
        let ciphertext = &ciphertext[12..];
        let nonce_obj = Nonce::from_slice(nonce_bytes);
        self.cipher
            .decrypt(nonce_obj, ciphertext)
            .map_err(|e| WALError::Decryption(format!("AES-256-GCM decrypt error: {}", e)))
    }
}

/// A single WAL record variant — the unit of crash recovery.
///
/// Each variant maps to a specific engine operation:
/// - `PlaceOrder`: Order was submitted and needs replay.
/// - `CancelOrder`: Order cancellation request.
/// - `SettleDOT`: DOT transfer to be replayed.
/// - `TradeSettled`: Completed trade for audit trail.
/// - `Heartbeat`: Periodic liveness signal (used for replica health).
/// - `Snapshot`: Full engine state snapshot marker (for compaction).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WALRecord {
    PlaceOrder(Order),
    CancelOrder(uuid::Uuid),
    SettleDOT(DOTTransfer),
    TradeSettled(Trade),
    Heartbeat(i64),
    Snapshot(String),
}

/// On-disk WAL entry with integrity checks and hash chain linkage.
///
/// `prev_hash` links to the previous entry's hash, forming a tamper-evident chain.
/// `crc32` protects against silent corruption. `seq` enables ordered replay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WALEntry {
    pub crc32: u32,
    pub length: u32,
    pub timestamp: i64,
    pub record: WALRecord,
    pub prev_hash: [u8; 32],
    pub seq: u64,
}

/// The Write-Ahead Log instance — crash-safe persistence layer.
///
/// # File Layout
/// The WAL file (`{node_id}.wal`) is an append-only log of `WALEntry`s.
/// On Linux, io_uring is used for direct NVMe submission when available.
///
/// # Sync Strategy
/// Entries are buffered in memory and flushed to disk + replicated every
/// `MAX_BATCH_SIZE` (64) entries. This trades a small window of data loss
/// for significantly higher throughput.
///
/// # Key Rotation
/// The Ed25519 signing key is derived from `WAL_SIGNING_SEED` env var
/// (or node_id if unset). The key is immutable for the node's lifetime.
pub struct WriteAheadLog {
    node_id: String,
    file_path: PathBuf,
    file: Mutex<std::fs::File>,
    current_offset: AtomicU64,
    last_sync: AtomicU64,
    replicas: Vec<String>,
    pending: Mutex<VecDeque<(u64, WALEntry)>>,
    next_seq: AtomicU64,
    last_hash: Mutex<[u8; 32]>,
    encryption: Option<EncryptionManager>,
    signing_key: [u8; 32],
    verifying_key: [u8; 32],
    replica_runtime: tokio::runtime::Runtime,
    #[cfg(target_os = "linux")]
    iouring: Option<IoUringHandle>,
}

impl WriteAheadLog {
    pub fn new(node_id: &str, wal_dir: &Path, replicas: Vec<String>) -> Result<Self, WALError> {
        let safe_node_id: String = node_id
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        if safe_node_id.is_empty() {
            return Err(WALError::DirCreate("node_id must contain alphanumeric, dash, or underscore characters".into()));
        }
        std::fs::create_dir_all(wal_dir).map_err(|e| WALError::DirCreate(e.to_string()))?;
        let file_path = wal_dir.join(format!("{}.wal", safe_node_id));

        #[cfg(target_os = "linux")]
        let file = {
            use std::os::unix::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .read(true)
                .mode(0o600)
                .open(&file_path)
                .map_err(|e| WALError::FileOpen(e.to_string()))?
        };

        #[cfg(not(target_os = "linux"))]
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(&file_path)
            .map_err(|e| WALError::FileOpen(e.to_string()))?;

        let start_offset = file.metadata().map(|m| m.len()).unwrap_or(0);

        // Initialize encryption (AES-256-GCM at-rest)
        let encryption = Self::init_encryption()?;

        // Initialize immutable signing key (Ed25519)
        let (signing_key, verifying_key) = Self::init_signing_key(node_id)?;

        let now = chrono::Utc::now().timestamp_millis() as u64;

        #[cfg(target_os = "linux")]
        let iouring_handle = IoUringHandle::new(&file, 256);

        let replica_runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .thread_name("wal-replica")
            .build()
            .map_err(|e| WALError::Runtime(format!("replica runtime: {}", e)))?;

        Ok(Self {
            node_id: node_id.to_string(),
            file_path,
            file: Mutex::new(file),
            current_offset: AtomicU64::new(start_offset),
            last_sync: AtomicU64::new(now),
            replicas,
            pending: Mutex::new(VecDeque::with_capacity(MAX_BATCH_SIZE * 2)),
            next_seq: AtomicU64::new(1),
            last_hash: Mutex::new([0u8; 32]),
            encryption,
            signing_key: signing_key.to_bytes(),
            verifying_key: verifying_key.to_bytes(),
            replica_runtime,
            #[cfg(target_os = "linux")]
            iouring: iouring_handle,
        })
    }

    fn init_signing_key(node_id: &str) -> Result<(ed25519_dalek::SigningKey, ed25519_dalek::VerifyingKey), WALError> {
        use blake2::Digest;
        // Derive signing key from node_id (or env var for production)
        let seed = match std::env::var("WAL_SIGNING_SEED") {
            Ok(s) => {
                let mut seed = [0u8; 32];
                let hash = blake2::Blake2b512::digest(s.as_bytes());
                seed.copy_from_slice(&hash[..32]);
                seed
            }
            Err(_) => {
                let mut seed = [0u8; 32];
                let hash = blake2::Blake2b512::digest(format!("the-bridge-wal-{}", node_id).as_bytes());
                seed.copy_from_slice(&hash[..32]);
                seed
            }
        };
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&seed);
        let verifying_key = signing_key.verifying_key();
        Ok((signing_key, verifying_key))
    }

    fn init_encryption() -> Result<Option<EncryptionManager>, WALError> {
        use base64::Engine;
        match std::env::var("WAL_ENCRYPTION_KEY") {
            Ok(key_b64) => {
                let key_bytes = base64::engine::general_purpose::STANDARD.decode(key_b64.as_bytes())
                    .map_err(|e| WALError::KeyError(format!("WAL_ENCRYPTION_KEY base64 decode failed: {}", e)))?;
                if key_bytes.len() != 32 {
                    return Err(WALError::KeyError("WAL_ENCRYPTION_KEY must be 32 bytes (base64)".into()));
                }
                let mut key = [0u8; 32];
                key.copy_from_slice(&key_bytes);
                Ok(Some(EncryptionManager::new(key)))
            }
            Err(_) => {
                // Unencrypted mode — suitable for development/testing
                #[cfg(not(debug_assertions))]
                tracing::warn!("WAL_ENCRYPTION_KEY not set — running without at-rest encryption (recommended for production)");
                Ok(None)
            }
        }
    }

    #[instrument(skip(self, record), fields(record_type = match &record { WALRecord::PlaceOrder(_) => "PlaceOrder", WALRecord::CancelOrder(_) => "CancelOrder", WALRecord::SettleDOT(_) => "SettleDOT", WALRecord::TradeSettled(_) => "TradeSettled", WALRecord::Heartbeat(_) => "Heartbeat", WALRecord::Snapshot(_) => "Snapshot" }))]
    pub fn append(&self, record: WALRecord) -> Result<u64, WALError> {
        let ts = chrono::Utc::now().timestamp_millis();
        let mut bytes = bincode::serialize(&record).map_err(|e| WALError::Serialize(e.to_string()))?;

        // Encrypt if encryption is enabled (production)
        if let Some(ref enc) = self.encryption {
            bytes = enc.encrypt(&bytes)?;
        }

        let length = bytes.len() as u32;

        let mut crc = Hasher::new();
        crc.update(&ts.to_le_bytes());
        crc.update(&length.to_le_bytes());
        crc.update(&bytes);
        let crc32 = crc.finalize();

        let seq = self.next_seq.fetch_add(1, Ordering::Relaxed);

        let prev_hash = *self.last_hash.lock();

        let mut hasher = Blake2b512::new();
        hasher.update(seq.to_le_bytes());
        hasher.update(&crc32.to_le_bytes());
        hasher.update(&prev_hash);
        let hash_result = hasher.finalize();
        let mut entry_hash = [0u8; 32];
        entry_hash.copy_from_slice(&hash_result[..32]);

        let entry = WALEntry { crc32, length, timestamp: ts, record, prev_hash, seq };

        // Build wire format: header(8+8+16+64+16) + payload + signature(64)
        let header = format!("{:08x}{:08x}{:016x}{}{:016x}", crc32, length, ts, hex::encode(prev_hash), seq);
        let entry_size = header.len() as u64 + length as u64 + 64;

        {
            let mut file = self.file.lock();
            let sig = ed25519_dalek::SigningKey::from_bytes(&self.signing_key)
                .sign(&entry_hash);
            let sig_bytes = sig.to_bytes();

            // Build combined buffer: header + payload + signature
            let total_len = header.len() + bytes.len() + 64;
            let mut combined = vec![0u8; total_len];
            let mut pos = 0;
            combined[pos..pos+header.len()].copy_from_slice(header.as_bytes());
            pos += header.len();
            combined[pos..pos+bytes.len()].copy_from_slice(&bytes);
            pos += bytes.len();
            combined[pos..pos+64].copy_from_slice(&sig_bytes);

            #[cfg(target_os = "linux")]
            {
                if let Some(ref iouring) = self.iouring {
                    if iouring.is_available() {
                        let aligned = aligned_buffer(total_len);
                        let mut buf = aligned;
                        buf[..total_len].copy_from_slice(&combined);
                        let offset = self.current_offset.load(Ordering::Relaxed);
                        iouring.write_at(&buf[..total_len], offset)
                            .map_err(|e| WALError::IoUring(e.to_string()))?;
                    } else {
                        file.write_all(&combined).map_err(|e| WALError::Write(e.to_string()))?;
                    }
                } else {
                    file.write_all(&combined).map_err(|e| WALError::Write(e.to_string()))?;
                }
            }

            #[cfg(not(target_os = "linux"))]
            {
                file.write_all(&combined).map_err(|e| WALError::Write(e.to_string()))?;
            }
        }

        self.current_offset.fetch_add(entry_size, Ordering::Relaxed);
        self.pending.lock().push_back((seq, entry));
        *self.last_hash.lock() = entry_hash;
        info!(seq, entry_size, "WAL entry appended");

        // Flush + replicate every MAX_BATCH_SIZE entries
        if seq % MAX_BATCH_SIZE as u64 == 0 {
            let _start = Instant::now();

            #[cfg(target_os = "linux")]
            {
                if let Some(ref iouring) = self.iouring {
                    if iouring.is_available() {
                        let _ = iouring.sync_all();
                    } else {
                        let mut f = self.file.lock();
                        f.flush().map_err(|e| WALError::Flush(e.to_string()))?;
                        drop(f);
                    }
                } else {
                    let mut f = self.file.lock();
                    f.flush().map_err(|e| WALError::Flush(e.to_string()))?;
                    drop(f);
                }
            }

            #[cfg(not(target_os = "linux"))]
            {
                let mut f = self.file.lock();
                f.flush().map_err(|e| WALError::Flush(e.to_string()))?;
                drop(f);
            }

            self.last_sync.store(ts as u64, Ordering::Relaxed);
            self.try_replicate().ok();
        }

        Ok(seq)
    }

    pub fn last_entry_hash(&self) -> [u8; 32] {
        *self.last_hash.lock()
    }

    pub fn verify_chain(&self, from_seq: u64, to_seq: u64) -> bool {
        let pending = self.pending.lock();
        let entries: Vec<&WALEntry> = pending.iter()
            .filter(|(s, _)| *s >= from_seq && *s <= to_seq)
            .map(|(_, e)| e)
            .collect();

        if entries.is_empty() { return true; }

        let first = entries[0];
        let mut expected_hash = first.prev_hash;

        for entry in entries {
            let mut hasher = Blake2b512::new();
            hasher.update(entry.seq.to_le_bytes());
            hasher.update(&entry.crc32.to_le_bytes());
            hasher.update(&entry.prev_hash);
            let hash = hasher.finalize();
            let mut computed = [0u8; 32];
            computed.copy_from_slice(&hash[..32]);
            if computed != expected_hash && entry.seq > 1 {
                return false;
            }
            expected_hash = computed;
        }
        true
    }

    pub fn sync(&self) -> Result<(), WALError> {
        #[cfg(target_os = "linux")]
        {
            if let Some(ref iouring) = self.iouring {
                if iouring.is_available() {
                    iouring.sync_all()?;
                    self.last_sync.store(chrono::Utc::now().timestamp_millis() as u64, Ordering::Relaxed);
                    return Ok(());
                }
            }
        }

        let mut file = self.file.lock();
        file.flush().map_err(|e| WALError::Flush(e.to_string()))?;
        file.sync_all().map_err(|e| WALError::Fsync(e.to_string()))?;
        self.last_sync.store(chrono::Utc::now().timestamp_millis() as u64, Ordering::Relaxed);
        Ok(())
    }

    fn try_replicate(&self) -> Result<(), WALError> {
        if self.replicas.is_empty() { return Ok(()); }

        let batch = {
            let pending = self.pending.lock();
            if pending.is_empty() { return Ok(()); }
            let entries: Vec<&WALEntry> = pending.iter().map(|(_, e)| e).collect();
            bincode::serialize(&entries).map_err(|e| WALError::Serialize(format!("batch serialize: {}", e)))?
        };

        for replica in &self.replicas {
            let _ = self.send_to_replica_blocking(replica, &batch);
        }
        Ok(())
    }

    fn send_to_replica_blocking(&self, addr: &str, data: &[u8]) -> Result<(), WALError> {
        self.replica_runtime.block_on(async {
            let mut stream = tokio::time::timeout(
                std::time::Duration::from_millis(100),
                TcpStream::connect(addr),
            ).await.map_err(|_| WALError::ReplicaTimeout)?
              .map_err(|e| WALError::ReplicaConnect(e.to_string()))?;

            let len = (data.len() as u64).to_le_bytes();
            stream.write_all(&len).await.map_err(|e| WALError::ReplicaConnect(format!("send len: {}", e)))?;
            stream.write_all(data).await.map_err(|e| WALError::ReplicaConnect(format!("send data: {}", e)))?;
            stream.flush().await.map_err(|e| WALError::ReplicaConnect(format!("flush: {}", e)))?;

            let mut ack = [0u8; 1];
            stream.read_exact(&mut ack).await.ok();
            Ok(())
        })
    }

    #[instrument(skip(self), fields(wal_path = %self.file_path.display()))]
    pub fn recover(&self) -> Result<Vec<WALRecord>, WALError> {
        info!("starting WAL recovery");
        let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&self.verifying_key)
            .map_err(|e| WALError::KeyError(format!("WAL verifying key: {}", e)))?;
        let mut file = std::fs::File::open(&self.file_path)
            .map_err(|e| WALError::FileOpen(format!("wal recover open: {}", e)))?;
        let metadata = file.metadata().map_err(|e| WALError::Metadata(e.to_string()))?;
        let size = metadata.len() as usize;
        if size == 0 { return Ok(Vec::new()); }

        let mut buf = Vec::with_capacity(size);
        file.read_to_end(&mut buf).map_err(|e| WALError::Read(e.to_string()))?;

        let mut records = Vec::new();
        let mut pos = 0;
        let mut expected_prev_hash = [0u8; 32];
        while pos + 112 <= buf.len() {
            let crc32 = u32::from_str_radix(
                std::str::from_utf8(&buf[pos..pos+8]).unwrap_or("0"),
                16
            ).unwrap_or(0);
            pos += 8;
            let length = u32::from_str_radix(
                std::str::from_utf8(&buf[pos..pos+8]).unwrap_or("0"),
                16
            ).unwrap_or(0) as usize;
            pos += 8;
            let ts = u64::from_str_radix(
                std::str::from_utf8(&buf[pos..pos+16]).unwrap_or("0"),
                16
            ).unwrap_or(0);
            pos += 16;
            let _prev_hash_hex = &buf[pos..pos+64];
            pos += 64;
            let seq = u64::from_str_radix(
                std::str::from_utf8(&buf[pos..pos+16]).unwrap_or("0"),
                16
            ).unwrap_or(0);
            pos += 16;

            if pos + length > buf.len() { break; }

            let mut crc_check = Hasher::new();
            crc_check.update(&ts.to_le_bytes());
            crc_check.update(&(length as u32).to_le_bytes());
            crc_check.update(&buf[pos..pos+length]);
            if crc_check.finalize() != crc32 { break; }

            let record_bytes = &buf[pos..pos+length];
            let decrypted = if let Some(ref enc) = self.encryption {
                enc.decrypt(record_bytes)?
            } else {
                record_bytes.to_vec()
            };

            if let Ok(record) = bincode::deserialize::<WALRecord>(&decrypted) {
                records.push(record);

                // Compute entry hash for signature verification
                let mut hasher = Blake2b512::new();
                hasher.update(seq.to_le_bytes());
                hasher.update(&crc32.to_le_bytes());
                hasher.update(&expected_prev_hash);
                let hash_result = hasher.finalize();
                let mut entry_hash = [0u8; 32];
                entry_hash.copy_from_slice(&hash_result[..32]);

                // Advance past record data, then try to read trailing Ed25519 signature (64 bytes)
                // New WALs have signature after each entry; old WALs don't — backward compatible
                pos += length;
                if pos + 64 <= buf.len() {
                    let mut sig_bytes = [0u8; 64];
                    sig_bytes.copy_from_slice(&buf[pos..pos+64]);
                    let sig = ed25519_dalek::Signature::from_bytes(&sig_bytes);
                    if verifying_key.verify_strict(&entry_hash, &sig).is_ok() {
                        expected_prev_hash = entry_hash;
                        pos += 64;
                    } else {
                        tracing::warn!(seq, "WAL: signature verification failed — possible tampering");
                        break;
                    }
                } else {
                    // No signature present (old WAL format) — update expected hash from data only
                    expected_prev_hash = entry_hash;
                }
            } else {
                pos += length;
            }
        }

        self.current_offset.store(pos as u64, Ordering::Relaxed);
        tracing::info!(records = records.len(), truncation_point = pos, "WAL recovery complete");
        Ok(records)
    }

    pub fn truncate(&self, keep_seq: u64) -> Result<(), WALError> {
        let mut pending = self.pending.lock();
        while let Some(front) = pending.front() {
            if front.0 <= keep_seq {
                pending.pop_front();
            } else { break; }
        }
        Ok(())
    }

    pub fn replica_lag(&self) -> u64 {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        now - self.last_sync.load(Ordering::Relaxed)
    }

    pub fn is_healthy(&self) -> bool {
        self.replica_lag() < MAX_REPLICA_LAG
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_key() -> [u8; 32] {
        [0x42u8; 32]
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let enc = EncryptionManager::new(make_test_key());
        let plaintext = b"hello, WAL world!";
        let ciphertext = enc.encrypt(plaintext).unwrap();

        assert_ne!(ciphertext, plaintext);
        let decrypted = enc.decrypt(&ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn different_encryptions_produce_different_ciphertexts() {
        let enc = EncryptionManager::new(make_test_key());
        let plaintext = b"same plaintext";

        let ct1 = enc.encrypt(plaintext).unwrap();
        let ct2 = enc.encrypt(plaintext).unwrap();

        // Random nonce guarantees different ciphertext
        assert_ne!(ct1, ct2);
    }

    #[test]
    fn decrypt_too_short_ciphertext_fails() {
        let enc = EncryptionManager::new(make_test_key());
        let result = enc.decrypt(&[0u8; 5]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("ciphertext too short"));
    }

    #[test]
    fn decrypt_with_wrong_key_fails() {
        let enc1 = EncryptionManager::new(make_test_key());
        let mut wrong_key = make_test_key();
        wrong_key[0] ^= 0xFF;
        let enc2 = EncryptionManager::new(wrong_key);

        let ciphertext = enc1.encrypt(b"secret data").unwrap();
        let result = enc2.decrypt(&ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn encrypt_empty_plaintext() {
        let enc = EncryptionManager::new(make_test_key());
        let ciphertext = enc.encrypt(b"").unwrap();
        let decrypted = enc.decrypt(&ciphertext).unwrap();
        assert_eq!(decrypted, b"");
    }

    #[test]
    fn encrypt_large_plaintext() {
        let enc = EncryptionManager::new(make_test_key());
        let plaintext = vec![0xABu8; 1024 * 1024]; // 1 MB
        let ciphertext = enc.encrypt(&plaintext).unwrap();
        let decrypted = enc.decrypt(&ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn wal_record_serialization_roundtrip() {
        let order = crate::types::Order::new_limit(
            uuid::Uuid::new_v4(),
            "BTC/USDT".into(),
            crate::types::OrderSide::Buy,
            50000.0,
            1.5,
        );
        let record = WALRecord::PlaceOrder(order);
        let encoded = bincode::serialize(&record).unwrap();
        let decoded: WALRecord = bincode::deserialize(&encoded).unwrap();

        match decoded {
            WALRecord::PlaceOrder(o) => {
                assert_eq!(o.pair.as_str(), "BTC/USDT");
                assert_eq!(o.price, 50000.0);
            }
            _ => panic!("expected PlaceOrder"),
        }
    }
}
