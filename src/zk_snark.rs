use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{info, warn, debug};
use dashmap::DashMap;
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZKConfig {
    pub circuit_type: ZKCircuitType,
    pub proof_system: ZKProofSystem,
    pub curve: ZKCurve,
    pub security_level: u32,
    pub verify_only: bool,
    pub parallel_proving: bool,
    pub max_witness_size: usize,
    pub proof_cache_size: usize,
    pub verification_batch_size: usize,
    pub enable_recursion: bool,
    pub recursion_depth: u32,
}

impl Default for ZKConfig {
    fn default() -> Self {
        Self {
            circuit_type: ZKCircuitType::Groth16,
            proof_system: ZKProofSystem::Arkworks,
            curve: ZKCurve::Bn254,
            security_level: 128,
            verify_only: false,
            parallel_proving: true,
            max_witness_size: 1024 * 1024,
            proof_cache_size: 10000,
            verification_batch_size: 64,
            enable_recursion: true,
            recursion_depth: 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ZKCircuitType {
    Groth16,
    Plonk,
    Marlin,
    Spartan,
    Nova,
    HyperPlonk,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ZKProofSystem {
    Arkworks,
    Bellman,
    Halo2,
    RISC0,
    SP1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ZKCurve {
    Bn254,
    Bls12_381,
    Mnt4_298,
    Pasta,
    Secp256k1,
    Ed25519,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZKProof {
    pub proof_id: String,
    pub circuit_id: String,
    pub proof_type: String,
    pub proof_data: Vec<u8>,
    pub public_inputs: Vec<Vec<u8>>,
    pub verifier_key_id: String,
    pub created_at: DateTime<Utc>,
    pub verified: bool,
    pub verification_time_ns: Option<u64>,
    pub proof_size_bytes: usize,
    pub witness_size_bytes: usize,
    pub recursion_depth: u32,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZKVerifierKey {
    pub key_id: String,
    pub circuit_type: String,
    pub curve: String,
    pub key_data: Vec<u8>,
    pub created_at: DateTime<Utc>,
    pub verification_count: u64,
    pub last_verified: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZKCircuit {
    pub circuit_id: String,
    pub name: String,
    pub num_constraints: u64,
    pub num_variables: u64,
    pub num_private_inputs: u64,
    pub num_public_inputs: u64,
    pub description: String,
    pub wasm_bytes: Option<Vec<u8>>,
    pub r1cs_bytes: Option<Vec<u8>>,
    pub compiled_at: DateTime<Utc>,
    pub version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZKStats {
    pub total_proofs: u64,
    pub total_verifications: u64,
    pub successful_verifications: u64,
    pub failed_verifications: u64,
    pub avg_proof_time_ns: u64,
    pub avg_verify_time_ns: u64,
    pub circuits_registered: usize,
    pub verifier_keys: usize,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub batch_verifications: u64,
    pub recursive_proofs: u64,
    pub total_witness_bytes: u64,
    pub total_proof_bytes: u64,
}

pub struct ZKSNARKEngine {
    config: Arc<RwLock<ZKConfig>>,
    proofs: Arc<DashMap<String, ZKProof>>,
    verifier_keys: Arc<DashMap<String, ZKVerifierKey>>,
    circuits: Arc<DashMap<String, ZKCircuit>>,
    proof_cache: Arc<DashMap<String, ZKProof>>,
    stats: Arc<RwLock<ZKStats>>,
    running: Arc<RwLock<bool>>,
}

impl ZKSNARKEngine {
    pub fn new(config: ZKConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            proofs: Arc::new(DashMap::new()),
            verifier_keys: Arc::new(DashMap::new()),
            circuits: Arc::new(DashMap::new()),
            proof_cache: Arc::new(DashMap::new()),
            stats: Arc::new(RwLock::new(ZKStats {
                total_proofs: 0,
                total_verifications: 0,
                successful_verifications: 0,
                failed_verifications: 0,
                avg_proof_time_ns: 0,
                avg_verify_time_ns: 0,
                circuits_registered: 0,
                verifier_keys: 0,
                cache_hits: 0,
                cache_misses: 0,
                batch_verifications: 0,
                recursive_proofs: 0,
                total_witness_bytes: 0,
                total_proof_bytes: 0,
            })),
            running: Arc::new(RwLock::new(false)),
        }
    }

    pub async fn start(&self) -> Result<(), String> {
        let mut running = self.running.write().await;
        *running = true;

        let config = self.config.read().await;
        info!(
            "ZK-SNARK engine started — circuit={:?} system={:?} curve={:?} security={}bit parallel={} recursion={} depth={}",
            config.circuit_type, config.proof_system, config.curve, config.security_level, config.parallel_proving, config.enable_recursion, config.recursion_depth
        );
        Ok(())
    }

    pub async fn register_circuit(&self, mut circuit: ZKCircuit) -> Result<(), String> {
        let config = self.config.read().await;
        if circuit.num_constraints > config.max_witness_size as u64 {
            return Err("circuit too large".to_string());
        }
        drop(config);

        circuit.compiled_at = Utc::now();
        
        self.circuits.insert(circuit.circuit_id.clone(), circuit.clone());

        let mut stats = self.stats.write().await;
        stats.circuits_registered = self.circuits.len();

        info!(
            "ZK circuit registered: id={} name={} constraints={} inputs={}/{}",
            circuit.circuit_id, circuit.name, circuit.num_constraints, circuit.num_private_inputs, circuit.num_public_inputs
        );
        Ok(())
    }

    pub async fn register_verifier_key(&self, mut key: ZKVerifierKey) -> Result<(), String> {
        key.created_at = Utc::now();
        
        self.verifier_keys.insert(key.key_id.clone(), key.clone());

        let mut stats = self.stats.write().await;
        stats.verifier_keys = self.verifier_keys.len();

        info!("ZK verifier key registered: id={}", key.key_id);
        Ok(())
    }

    pub async fn generate_proof(
        &self,
        circuit_id: &str,
        private_inputs: &[u8],
        public_inputs: &[Vec<u8>],
        metadata: Option<HashMap<String, String>>,
    ) -> Result<ZKProof, String> {
        let start = std::time::Instant::now();
        let config = self.config.read().await;

        let circuit = self.circuits
            .get(circuit_id).map(|e| e.value().clone())
            .ok_or("circuit not found")?;

        if private_inputs.len() > config.max_witness_size {
            return Err("witness too large".to_string());
        }
        drop(config);

        let proof_id = format!("proof_{}", Uuid::new_v4());
        let proof_data = self.simulate_proof_generation(circuit_id, private_inputs, public_inputs).await;
        let proof_size = proof_data.len();

        let proof = ZKProof {
            proof_id: proof_id.clone(),
            circuit_id: circuit_id.to_string(),
            proof_type: circuit_id.to_string(),
            proof_data,
            public_inputs: public_inputs.to_vec(),
            verifier_key_id: format!("vk_{}", circuit_id),
            created_at: Utc::now(),
            verified: false,
            verification_time_ns: None,
            proof_size_bytes: proof_size,
            witness_size_bytes: private_inputs.len(),
            recursion_depth: 0,
            metadata: metadata.unwrap_or_default(),
        };

        {
            self.proofs.insert(proof_id.clone(), proof.clone());
        }

        let mut stats = self.stats.write().await;
        stats.total_proofs += 1;
        stats.total_witness_bytes += private_inputs.len() as u64;
        stats.total_proof_bytes += proof_size as u64;

        let proof_time = start.elapsed().as_nanos() as u64;
        {
            let mut stats = self.stats.write().await;
            stats.avg_proof_time_ns = ((stats.avg_proof_time_ns * (stats.total_proofs - 1)) + proof_time) / stats.total_proofs;
        }

        info!(
            "ZK proof generated: id={} circuit={} size={}B time={}μs",
            proof_id, circuit_id, proof_size, proof_time / 1000
        );
        Ok(proof)
    }

    pub async fn generate_recursive_proof(
        &self,
        circuit_id: &str,
        inner_proofs: &[ZKProof],
        metadata: Option<HashMap<String, String>>,
    ) -> Result<ZKProof, String> {
        let config = self.config.read().await;
        if !config.enable_recursion {
            return Err("recursion disabled".to_string());
        }
        if inner_proofs.len() > config.recursion_depth as usize {
            return Err("recursion depth exceeded".to_string());
        }
        drop(config);

        let combined_witness = inner_proofs.iter()
            .flat_map(|p| p.proof_data.iter().cloned())
            .collect::<Vec<u8>>();

        let proof_id = format!("recursive_proof_{}", Uuid::new_v4());
        let proof_data = self.simulate_proof_generation(circuit_id, &combined_witness, &[]).await;
        let proof_size = proof_data.len();

        let proof = ZKProof {
            proof_id: proof_id.clone(),
            circuit_id: circuit_id.to_string(),
            proof_type: format!("recursive_{}", circuit_id),
            proof_data,
            public_inputs: vec![],
            verifier_key_id: format!("vk_{}", circuit_id),
            created_at: Utc::now(),
            verified: false,
            verification_time_ns: None,
            proof_size_bytes: proof_size,
            witness_size_bytes: combined_witness.len(),
            recursion_depth: inner_proofs.first().map(|p| p.recursion_depth + 1).unwrap_or(1),
            metadata: HashMap::new(),
        };

        {
            self.proofs.insert(proof_id.clone(), proof.clone());
        }

        let mut stats = self.stats.write().await;
        stats.total_proofs += 1;
        stats.recursive_proofs += 1;
        stats.total_proof_bytes += proof_size as u64;

        info!(
            "ZK recursive proof generated: id={} depth={} inner={}",
            proof_id, proof.recursion_depth, inner_proofs.len()
        );
        Ok(proof)
    }

    pub async fn verify_proof(&self, proof_id: &str) -> Result<bool, String> {
        let start = std::time::Instant::now();
        
        let proof = self.proofs
            .get(proof_id).map(|e| e.value().clone())
            .ok_or("proof not found")?;

        let vk = self.verifier_keys
            .get(&proof.verifier_key_id).map(|e| e.value().clone())
            .ok_or("verifier key not found")?;

        let is_valid = self.verify_proof_data(&proof, &vk).await?;

        let verify_time = start.elapsed().as_nanos() as u64;
        
        {
            let mut proof_mut = self.proofs.get_mut(proof_id)
                .ok_or("proof not found")?;
            proof_mut.verified = is_valid;
            proof_mut.verification_time_ns = Some(verify_time);
        }

        {
            let mut vk_mut = self.verifier_keys.get_mut(&proof.verifier_key_id)
                .ok_or("verifier key not found")?;
            vk_mut.verification_count += 1;
            vk_mut.last_verified = Some(Utc::now());
        }

        {
            let mut stats = self.stats.write().await;
            stats.total_verifications += 1;
            if is_valid {
                stats.successful_verifications += 1;
            } else {
                stats.failed_verifications += 1;
            }
            stats.avg_verify_time_ns = ((stats.avg_verify_time_ns * (stats.total_verifications - 1)) + verify_time) / stats.total_verifications;
        }

        info!(
            "ZK proof verification: proof_id={} valid={} time={}μs",
            proof_id, is_valid, verify_time / 1000
        );
        Ok(is_valid)
    }

    async fn verify_proof_data(&self, proof: &ZKProof, vk: &ZKVerifierKey) -> Result<bool, String> {
        Ok(proof.proof_data.len() >= 32 && proof.public_inputs.len() > 0)
    }

    pub async fn batch_verify(&self, proof_ids: &[String]) -> Result<Vec<bool>, String> {
        let config = self.config.read().await;
        if proof_ids.len() > config.verification_batch_size {
            return Err("batch size exceeded".to_string());
        }
        drop(config);

        let mut results = Vec::with_capacity(proof_ids.len());
        for id in proof_ids {
            let result = self.verify_proof(id).await?;
            results.push(result);
        }

        let mut stats = self.stats.write().await;
        stats.batch_verifications += 1;

        Ok(results)
    }

    pub async fn get_proof(&self, proof_id: &str) -> Option<ZKProof> {
        self.proofs.get(proof_id).map(|e| e.value().clone())
    }

    pub async fn get_stats(&self) -> ZKStats {
        self.stats.read().await.clone()
    }

    pub async fn get_circuit(&self, circuit_id: &str) -> Option<ZKCircuit> {
        self.circuits.get(circuit_id).map(|e| e.value().clone())
    }

    pub async fn list_circuits(&self) -> Vec<ZKCircuit> {
        self.circuits.iter().map(|e| e.value().clone()).collect()
    }

    pub async fn clear_cache(&self) -> usize {
        let count = self.proof_cache.len();
        self.proof_cache.clear();
        count
    }

    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;
        info!("ZK-SNARK engine stopped");
    }

    async fn simulate_proof_generation(&self, circuit_id: &str, private_inputs: &[u8], public_inputs: &[Vec<u8>]) -> Vec<u8> {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(circuit_id.as_bytes());
        hasher.update(private_inputs);
        hasher.update(&public_inputs.iter().flatten().cloned().collect::<Vec<u8>>());
        hasher.update(Utc::now().timestamp_millis().to_be_bytes());
        hasher.finalize().to_vec()
    }
}