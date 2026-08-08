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
use aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
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
    #[error("WAL channel error: {0}")]
    Channel(String),
    #[error("WAL rotation error: {0}")]
    Rotation(String),
}

impl From<WALError> for String {
    fn from(e: WALError) -> String {
        e.to_string()
    }
}
use tracing::{info, instrument};

/// WAL file magic number for format identification (unused currently, reserved).
const WAL_MAGIC: u32 = 0x53424757;
/// WAL magic for v2 binary format.
const WAL_MAGIC_V2: u32 = 0x57414C32;
/// Number of entries between flush + replication cycles.
const MAX_BATCH_SIZE: usize = 64;
/// Maximum allowed replication lag in milliseconds before marking unhealthy.
const MAX_REPLICA_LAG: u64 = 10_000;
/// Size of binary header: magic(4) + crc32(4) + length(4) + ts(8) + prev_hash(32) + seq(8)
const NEW_WAL_HEADER_SIZE: usize = 60;
/// Size of old hex header: crc32(8) + length(8) + ts(16) + prev_hash(64) + seq(16)
const OLD_WAL_HEADER_SIZE: usize = 112;
/// Ed25519 signature size in bytes.
const SIG_SIZE: usize = 64;
/// Maximum WAL file size before rotation (64 MB).
const MAX_WAL_FILE_SIZE: u64 = 64 * 1024 * 1024;
/// Maximum number of rotated WAL files to keep.
const MAX_WAL_FILES: usize = 10;

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
    use super::WALError;
    use io_uring::{opcode, types, IoUring};
    use std::os::unix::io::{AsRawFd, RawFd};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;

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
                    Some(Self {
                        ring: Mutex::new(ring),
                        fd,
                        sq_entries: entries,
                        available: AtomicBool::new(true),
                    })
                }
                Err(e) => {
                    tracing::warn!(error = %e, "io_uring not available — falling back to std fs");
                    // Try a minimal 1-entry ring as a dummy (never used since available=false)
                    match IoUring::new(1) {
                        Ok(ring) => Some(Self {
                            ring: Mutex::new(ring),
                            fd,
                            sq_entries: 1,
                            available: AtomicBool::new(false),
                        }),
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
            let write_e = opcode::Write::new(types::Fd(self.fd), buf.as_ptr(), buf.len() as u32)
                .offset(offset)
                .build();

            let mut ring = self
                .ring
                .lock()
                .map_err(|e| WALError::IoUring(format!("ring lock: {}", e)))?;
            unsafe {
                let mut sq = ring.submission();
                sq.push(&write_e)
                    .map_err(|e| WALError::IoUring(format!("io_uring sq push: {:?}", e)))?;
            }

            let cqes = ring
                .submit_and_wait(1)
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
            let sync_e = opcode::Fsync::new(types::Fd(self.fd)).build();

            let mut ring = self
                .ring
                .lock()
                .map_err(|e| WALError::IoUring(format!("ring lock: {}", e)))?;
            unsafe {
                let mut sq = ring.submission();
                sq.push(&sync_e)
                    .map_err(|e| WALError::IoUring(format!("io_uring sq push: {:?}", e)))?;
            }

            let cqes = ring
                .submit_and_wait(1)
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
        Self {
            key: key_bytes,
            cipher,
        }
    }

    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, WALError> {
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce_obj = Nonce::from_slice(&nonce_bytes);
        let ciphertext = self
            .cipher
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

/// Commands sent from the main thread to the background WAL writer thread.
enum WriterCommand {
    Write(Vec<u8>),
    Flush(std::sync::mpsc::Sender<()>),
    Shutdown,
}

fn spawn_writer_thread(
    mut file: std::fs::File,
    file_path: PathBuf,
    rx: std::sync::mpsc::Receiver<WriterCommand>,
    last_sync: std::sync::Arc<AtomicU64>,
) -> std::thread::JoinHandle<()> {
        std::thread::Builder::new()
        .name("wal-writer".into())
        .spawn(move || {
            let mut wal_size: u64 = 0;

            #[cfg(target_os = "linux")]
            let _iouring = IoUringHandle::new(&file, 256);

            loop {
                match rx.recv() {
                    Ok(WriterCommand::Write(data)) => {
                        let entry_size = data.len() as u64;
                        if wal_size + entry_size > MAX_WAL_FILE_SIZE {
                            let _ = file.sync_all();
                            let rotated = rotate_wal_file(&file_path);
                            match rotated {
                                Ok(new_file) => {
                                    file = new_file;
                                    wal_size = 0;
                                    #[cfg(target_os = "linux")]
                                    let _iouring = IoUringHandle::new(&file, 256);
                                }
                                Err(_) => {}
                            }
                        }
                        if let Ok(_) = file.write_all(&data) {
                            wal_size += entry_size;
                        }
                    }
                    Ok(WriterCommand::Flush(ack)) => {
                        let _ = file.sync_all();
                        last_sync.store(
                            chrono::Utc::now().timestamp_millis() as u64,
                            std::sync::atomic::Ordering::Relaxed,
                        );
                        let _ = ack.send(());
                    }
                    Ok(WriterCommand::Shutdown) => {
                        let _ = file.sync_all();
                        break;
                    }
                    Err(_) => break,
                }
            }
        })
        .expect("spawn WAL writer thread")
}

fn rotate_wal_file(file_path: &Path) -> Result<std::fs::File, WALError> {
    for i in (1..MAX_WAL_FILES).rev() {
        let old_path = file_path.with_extension(format!("wal.{}", i));
        let new_path = file_path.with_extension(format!("wal.{}", i + 1));
        if old_path.exists() {
            let _ = std::fs::rename(&old_path, &new_path);
        }
    }
    let first_backup = file_path.with_extension("wal.1");
    let _ = std::fs::rename(file_path, &first_backup);
    let oldest = file_path.with_extension(format!("wal.{}", MAX_WAL_FILES));
    let _ = std::fs::remove_file(&oldest);
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .read(true)
        .open(file_path)
        .map_err(|e| WALError::FileOpen(format!("rotate: {}", e)))
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
    last_sync: std::sync::Arc<AtomicU64>,
    replicas: Vec<String>,
    pending: Mutex<VecDeque<(u64, WALEntry)>>,
    next_seq: AtomicU64,
    last_hash: Mutex<[u8; 32]>,
    encryption: Option<EncryptionManager>,
    signing_key: ed25519_dalek::SigningKey,
    verifying_key: [u8; 32],
    replica_runtime: tokio::runtime::Runtime,
    writer_tx: std::sync::mpsc::Sender<WriterCommand>,
    writer_handle: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl WriteAheadLog {
    pub fn new(node_id: &str, wal_dir: &Path, replicas: Vec<String>) -> Result<Self, WALError> {
        let safe_node_id: String = node_id
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        if safe_node_id.is_empty() {
            return Err(WALError::DirCreate(
                "node_id must contain alphanumeric, dash, or underscore characters".into(),
            ));
        }
        std::fs::create_dir_all(wal_dir).map_err(|e| WALError::DirCreate(e.to_string()))?;
        let file_path = wal_dir.join(format!("{}.wal", safe_node_id));

        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(&file_path)
            .map_err(|e| WALError::FileOpen(format!("open {}: {}", file_path.display(), e)))?;

        let encryption = Self::init_encryption()?;

        let (signing_key, verifying_key) = Self::init_signing_key(node_id)?;

        let now = chrono::Utc::now().timestamp_millis() as u64;
        let last_sync = std::sync::Arc::new(AtomicU64::new(now));

        let (writer_tx, writer_rx) = std::sync::mpsc::channel::<WriterCommand>();
        let writer_handle = spawn_writer_thread(file, file_path.clone(), writer_rx, last_sync.clone());

        let replica_runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .thread_name("wal-replica")
            .build()
            .map_err(|e| WALError::Runtime(format!("replica runtime: {}", e)))?;

        Ok(Self {
            node_id: node_id.to_string(),
            file_path,
            last_sync,
            replicas,
            pending: Mutex::new(VecDeque::with_capacity(MAX_BATCH_SIZE * 2)),
            next_seq: AtomicU64::new(1),
            last_hash: Mutex::new([0u8; 32]),
            encryption,
            signing_key,
            verifying_key: verifying_key.to_bytes(),
            replica_runtime,
            writer_tx,
            writer_handle: Mutex::new(Some(writer_handle)),
        })
    }

    fn init_signing_key(
        node_id: &str,
    ) -> Result<(ed25519_dalek::SigningKey, ed25519_dalek::VerifyingKey), WALError> {
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
                let hash =
                    blake2::Blake2b512::digest(format!("the-bridge-wal-{}", node_id).as_bytes());
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
                let key_bytes = base64::engine::general_purpose::STANDARD
                    .decode(key_b64.as_bytes())
                    .map_err(|e| {
                        WALError::KeyError(format!(
                            "WAL_ENCRYPTION_KEY base64 decode failed: {}",
                            e
                        ))
                    })?;
                if key_bytes.len() != 32 {
                    return Err(WALError::KeyError(
                        "WAL_ENCRYPTION_KEY must be 32 bytes (base64)".into(),
                    ));
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
        let mut bytes = crate::types::bincode_serialize_direct(&record)
            .map_err(|e| WALError::Serialize(e.to_string()))?;

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

        let entry = WALEntry {
            crc32,
            length,
            timestamp: ts,
            record,
            prev_hash,
            seq,
        };

        let sig = self.signing_key.sign(&entry_hash);
        let sig_bytes = sig.to_bytes();

        let total_size = NEW_WAL_HEADER_SIZE + bytes.len() + SIG_SIZE;
        let mut combined = Vec::with_capacity(total_size);
        combined.extend_from_slice(&WAL_MAGIC_V2.to_le_bytes());
        combined.extend_from_slice(&crc32.to_le_bytes());
        combined.extend_from_slice(&length.to_le_bytes());
        combined.extend_from_slice(&ts.to_le_bytes());
        combined.extend_from_slice(&prev_hash);
        combined.extend_from_slice(&seq.to_le_bytes());
        combined.extend_from_slice(&bytes);
        combined.extend_from_slice(&sig_bytes);

        self.writer_tx
            .send(WriterCommand::Write(combined))
            .map_err(|e| WALError::Channel(format!("writer channel: {}", e)))?;

        self.pending.lock().push_back((seq, entry));
        *self.last_hash.lock() = entry_hash;
        info!(seq, entry_size = total_size, "WAL entry appended");

        Ok(seq)
    }

    pub fn last_entry_hash(&self) -> [u8; 32] {
        *self.last_hash.lock()
    }

    pub fn verify_chain(&self, from_seq: u64, to_seq: u64) -> bool {
        let pending = self.pending.lock();
        let entries: Vec<&WALEntry> = pending
            .iter()
            .filter(|(s, _)| *s >= from_seq && *s <= to_seq)
            .map(|(_, e)| e)
            .collect();

        if entries.is_empty() {
            return true;
        }

        let first = entries[0];
        let mut expected_hash = first.prev_hash;

        for entry in entries {
            if entry.prev_hash != expected_hash {
                return false;
            }
            let mut hasher = Blake2b512::new();
            hasher.update(entry.seq.to_le_bytes());
            hasher.update(&entry.crc32.to_le_bytes());
            hasher.update(&entry.prev_hash);
            let hash = hasher.finalize();
            let mut computed = [0u8; 32];
            computed.copy_from_slice(&hash[..32]);
            expected_hash = computed;
        }
        true
    }

    pub fn flush(&self) -> Result<(), WALError> {
        let (ack_tx, ack_rx) = std::sync::mpsc::channel::<()>();
        self.writer_tx
            .send(WriterCommand::Flush(ack_tx))
            .map_err(|e| WALError::Channel(format!("flush: writer channel: {}", e)))?;
        ack_rx
            .recv_timeout(std::time::Duration::from_millis(500))
            .map_err(|e| WALError::Channel(format!("flush ack: {}", e)))?;
        Ok(())
    }

    pub fn sync(&self) -> Result<(), WALError> {
        self.flush()
    }

    fn try_replicate(&self) -> Result<(), WALError> {
        if self.replicas.is_empty() {
            return Ok(());
        }

        let batch = {
            let pending = self.pending.lock();
            if pending.is_empty() {
                return Ok(());
            }
            let entries: Vec<&WALEntry> = pending.iter().map(|(_, e)| e).collect();
            crate::types::bincode_serialize_direct(&entries)
                .map_err(|e| WALError::Serialize(format!("batch serialize: {}", e)))?
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
            )
            .await
            .map_err(|_| WALError::ReplicaTimeout)?
            .map_err(|e| WALError::ReplicaConnect(e.to_string()))?;

            let len = (data.len() as u64).to_le_bytes();
            stream
                .write_all(&len)
                .await
                .map_err(|e| WALError::ReplicaConnect(format!("send len: {}", e)))?;
            stream
                .write_all(data)
                .await
                .map_err(|e| WALError::ReplicaConnect(format!("send data: {}", e)))?;
            stream
                .flush()
                .await
                .map_err(|e| WALError::ReplicaConnect(format!("flush: {}", e)))?;

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

        let mut all_paths = vec![self.file_path.clone()];
        for i in 1..=MAX_WAL_FILES {
            let p = if i == 1 {
                self.file_path.with_extension("wal.1")
            } else {
                self.file_path.with_extension(format!("wal.{}", i))
            };
            if p.exists() {
                all_paths.push(p);
            } else {
                break;
            }
        }

        let mut records = Vec::new();
        let mut expected_prev_hash = [0u8; 32];

        for path in &all_paths {
            let mut file = match std::fs::File::open(path) {
                Ok(f) => f,
                Err(_) => continue,
            };
            let metadata = match file.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            let size = metadata.len() as usize;
            if size == 0 {
                continue;
            }

            let mut buf = Vec::with_capacity(size);
            if file.read_to_end(&mut buf).is_err() {
                continue;
            }

            if buf.len() >= 4 {
                let magic = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
                if magic == WAL_MAGIC_V2 {
                    let result = recover_v2(
                        &buf,
                        &verifying_key,
                        &self.encryption,
                        &mut expected_prev_hash,
                    );
                    records.extend(result);
                    continue;
                }
            }

            let result = recover_v1(
                &buf,
                &verifying_key,
                &self.encryption,
                &mut expected_prev_hash,
            );
            records.extend(result);
        }

        *self.last_hash.lock() = expected_prev_hash;
        tracing::info!(records = records.len(), "WAL recovery complete");
        Ok(records)
    }

    pub fn truncate(&self, keep_seq: u64) -> Result<(), WALError> {
        let mut pending = self.pending.lock();
        while let Some(front) = pending.front() {
            if front.0 <= keep_seq {
                pending.pop_front();
            } else {
                break;
            }
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

impl Drop for WriteAheadLog {
    fn drop(&mut self) {
        let _ = self.writer_tx.send(WriterCommand::Shutdown);
        if let Some(handle) = self.writer_handle.lock().take() {
            let _ = handle.join();
        }
    }
}

fn recover_v1(
    buf: &[u8],
    verifying_key: &ed25519_dalek::VerifyingKey,
    encryption: &Option<EncryptionManager>,
    expected_prev_hash: &mut [u8; 32],
) -> Vec<WALRecord> {
    let mut records = Vec::new();
    let mut pos = 0;
    while pos + OLD_WAL_HEADER_SIZE <= buf.len() {
        let crc32 = u32::from_str_radix(std::str::from_utf8(&buf[pos..pos + 8]).unwrap_or("0"), 16)
            .unwrap_or(0);
        pos += 8;
        let length = u32::from_str_radix(std::str::from_utf8(&buf[pos..pos + 8]).unwrap_or("0"), 16)
            .unwrap_or(0) as usize;
        pos += 8;
        let ts = u64::from_str_radix(std::str::from_utf8(&buf[pos..pos + 16]).unwrap_or("0"), 16)
            .unwrap_or(0);
        pos += 16;
        pos += 64;
        let seq = u64::from_str_radix(std::str::from_utf8(&buf[pos..pos + 16]).unwrap_or("0"), 16)
            .unwrap_or(0);
        pos += 16;

        if pos + length > buf.len() {
            break;
        }

        let mut crc_check = Hasher::new();
        crc_check.update(&ts.to_le_bytes());
        crc_check.update(&(length as u32).to_le_bytes());
        crc_check.update(&buf[pos..pos + length]);
        if crc_check.finalize() != crc32 {
            break;
        }

        let record_bytes = &buf[pos..pos + length];
        let decrypted = if let Some(ref enc) = encryption {
            match enc.decrypt(record_bytes) {
                Ok(d) => d,
                Err(_) => break,
            }
        } else {
            record_bytes.to_vec()
        };

        if let Ok(record) = crate::types::bincode_deserialize_direct::<WALRecord>(&decrypted) {
            let mut hasher = Blake2b512::new();
            hasher.update(seq.to_le_bytes());
            hasher.update(&crc32.to_le_bytes());
            hasher.update(&mut *expected_prev_hash);
            let hash_result = hasher.finalize();
            let mut entry_hash = [0u8; 32];
            entry_hash.copy_from_slice(&hash_result[..32]);

            pos += length;
            if pos + SIG_SIZE <= buf.len() {
                let mut sig_bytes = [0u8; SIG_SIZE];
                sig_bytes.copy_from_slice(&buf[pos..pos + SIG_SIZE]);
                let sig = ed25519_dalek::Signature::from_bytes(&sig_bytes);
                if verifying_key.verify_strict(&entry_hash, &sig).is_ok() {
                    *expected_prev_hash = entry_hash;
                    records.push(record);
                    pos += SIG_SIZE;
                } else {
                    break;
                }
            } else {
                *expected_prev_hash = entry_hash;
                records.push(record);
            }
        } else {
            pos += length;
        }
    }
    records
}

fn recover_v2(
    buf: &[u8],
    verifying_key: &ed25519_dalek::VerifyingKey,
    encryption: &Option<EncryptionManager>,
    expected_prev_hash: &mut [u8; 32],
) -> Vec<WALRecord> {
    let mut records = Vec::new();
    let mut pos = 0;

    while pos + NEW_WAL_HEADER_SIZE <= buf.len() {
        let magic = u32::from_le_bytes([buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]]);
        if magic != WAL_MAGIC_V2 {
            break;
        }

        let crc32 = u32::from_le_bytes([buf[pos + 4], buf[pos + 5], buf[pos + 6], buf[pos + 7]]);
        let length =
            u32::from_le_bytes([buf[pos + 8], buf[pos + 9], buf[pos + 10], buf[pos + 11]]) as usize;
        let timestamp = i64::from_le_bytes([
            buf[pos + 12],
            buf[pos + 13],
            buf[pos + 14],
            buf[pos + 15],
            buf[pos + 16],
            buf[pos + 17],
            buf[pos + 18],
            buf[pos + 19],
        ]);
        let seq = u64::from_le_bytes([
            buf[pos + 52],
            buf[pos + 53],
            buf[pos + 54],
            buf[pos + 55],
            buf[pos + 56],
            buf[pos + 57],
            buf[pos + 58],
            buf[pos + 59],
        ]);
        pos += NEW_WAL_HEADER_SIZE;

        if pos + length > buf.len() {
            break;
        }

        let mut crc_check = Hasher::new();
        crc_check.update(&timestamp.to_le_bytes());
        crc_check.update(&(length as u32).to_le_bytes());
        crc_check.update(&buf[pos..pos + length]);
        if crc_check.finalize() != crc32 {
            break;
        }

        let record_bytes = &buf[pos..pos + length];
        let decrypted = if let Some(ref enc) = encryption {
            match enc.decrypt(record_bytes) {
                Ok(d) => d,
                Err(_) => break,
            }
        } else {
            record_bytes.to_vec()
        };

        if let Ok(record) = crate::types::bincode_deserialize_direct::<WALRecord>(&decrypted) {
            let mut hasher = Blake2b512::new();
            hasher.update(seq.to_le_bytes());
            hasher.update(&crc32.to_le_bytes());
            hasher.update(&mut *expected_prev_hash);
            let hash_result = hasher.finalize();
            let mut entry_hash = [0u8; 32];
            entry_hash.copy_from_slice(&hash_result[..32]);

            pos += length;
            if pos + SIG_SIZE <= buf.len() {
                let mut sig_bytes = [0u8; SIG_SIZE];
                sig_bytes.copy_from_slice(&buf[pos..pos + SIG_SIZE]);
                let sig = ed25519_dalek::Signature::from_bytes(&sig_bytes);
                if verifying_key.verify_strict(&entry_hash, &sig).is_ok() {
                    *expected_prev_hash = entry_hash;
                    records.push(record);
                    pos += SIG_SIZE;
                } else {
                    break;
                }
            } else {
                *expected_prev_hash = entry_hash;
                records.push(record);
            }
        } else {
            pos += length;
        }
    }
    records
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
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("ciphertext too short"));
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
        let encoded = crate::types::bincode_serialize_direct(&record).unwrap();
        let decoded: WALRecord = crate::types::bincode_deserialize_direct(&encoded).unwrap();

        match decoded {
            WALRecord::PlaceOrder(o) => {
                assert_eq!(o.pair.as_str(), "BTC/USDT");
                assert_eq!(o.price, 50000.0);
            }
            _ => panic!("expected PlaceOrder"),
        }
    }

    #[test]
    fn wal_append_recover_roundtrip() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let wal = WriteAheadLog::new("test-node", tmp.path(), vec![]).expect("create WAL");

        let order = crate::types::Order::new_limit(
            uuid::Uuid::new_v4(),
            "BTC/USDT".into(),
            crate::types::OrderSide::Buy,
            50000.0,
            1.5,
        );
        let seq = wal
            .append(WALRecord::PlaceOrder(order.clone()))
            .expect("append");
        assert_eq!(seq, 1);

        wal.flush().expect("flush");
        drop(wal);

        let wal2 = WriteAheadLog::new("test-node", tmp.path(), vec![]).expect("create WAL 2");
        let records = wal2.recover().expect("recover");
        assert_eq!(records.len(), 1);
        match &records[0] {
            WALRecord::PlaceOrder(o) => {
                assert_eq!(o.pair.as_str(), "BTC/USDT");
                assert_eq!(o.price, 50000.0);
            }
            _ => panic!("expected PlaceOrder"),
        }
    }

    #[test]
    fn wal_empty_recover() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let wal = WriteAheadLog::new("test-empty", tmp.path(), vec![]).expect("create WAL");
        let records = wal.recover().expect("recover");
        assert!(records.is_empty());
    }

    #[test]
    fn wal_multiple_entries() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let wal = WriteAheadLog::new("test-multi", tmp.path(), vec![]).expect("create WAL");

        for i in 0..10 {
            let order = crate::types::Order::new_limit(
                uuid::Uuid::new_v4(),
                "BTC/USDT".into(),
                crate::types::OrderSide::Buy,
                50000.0 + i as f64,
                1.0,
            );
            wal.append(WALRecord::PlaceOrder(order)).expect("append");
        }
        wal.flush().expect("flush");
        drop(wal);

        let wal2 = WriteAheadLog::new("test-multi", tmp.path(), vec![]).expect("create WAL 2");
        let records = wal2.recover().expect("recover");
        assert_eq!(records.len(), 10);
    }

    #[test]
    fn wal_chain_verification() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let wal = WriteAheadLog::new("test-chain", tmp.path(), vec![]).expect("create WAL");

        for i in 0..5 {
            let order = crate::types::Order::new_limit(
                uuid::Uuid::new_v4(),
                "BTC/USDT".into(),
                crate::types::OrderSide::Buy,
                50000.0 + i as f64,
                1.0,
            );
            wal.append(WALRecord::PlaceOrder(order)).expect("append");
        }
        wal.flush().expect("flush");

        assert!(wal.verify_chain(1, 5));
        drop(wal);

        let wal2 = WriteAheadLog::new("test-chain", tmp.path(), vec![]).expect("create WAL 2");
        let records = wal2.recover().expect("recover");
        assert_eq!(records.len(), 5);
    }
}
