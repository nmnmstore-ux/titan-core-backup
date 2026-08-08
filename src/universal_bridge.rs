use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Universal Bridge — connects THE-BRIDGE as master node to any subsidiary project.
/// Each project gets an endpoint + auth key + capability profile.
/// Data/instructions can be forwarded to any connected project.
#[derive(Clone)]
pub struct UniversalBridge {
    projects: Arc<DashMap<String, ProjectConnection>>,
    total_forwarded: Arc<AtomicU64>,
    total_errors: Arc<AtomicU64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConnection {
    pub name: String,
    pub endpoint_url: String,
    pub auth_key_hmac: String,
    pub capabilities: Vec<Capability>,
    pub status: ProjectStatus,
    pub added_at: i64,
    pub last_seen: i64,
    pub total_routed: u64,
    pub total_errors: u64,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProjectStatus {
    Online,
    Offline,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Capability {
    ReceiveOrders,
    ReceiveSettlements,
    ReceiveISO20022,
    ReceiveGhostCommands,
    SendData,
    Bidirectional,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeCommand {
    pub source: String,
    pub command_type: String,
    pub payload: serde_json::Value,
    pub timestamp_ns: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeResponse {
    pub project: String,
    pub status: String,
    pub result: serde_json::Value,
    pub duration_us: u64,
}

impl UniversalBridge {
    pub fn new() -> Self {
        Self {
            projects: Arc::new(DashMap::new()),
            total_forwarded: Arc::new(AtomicU64::new(0)),
            total_errors: Arc::new(AtomicU64::new(0)),
        }
    }

    // ===== Project Registration =====

    pub fn register_project(
        &self,
        name: &str,
        endpoint_url: &str,
        auth_key: &str,
        capabilities: Vec<Capability>,
        description: &str,
    ) {
        use std::time::SystemTime;
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        self.projects.insert(
            name.to_string(),
            ProjectConnection {
                name: name.to_string(),
                endpoint_url: endpoint_url.to_string(),
                auth_key_hmac: self.hash_key(auth_key),
                capabilities,
                status: ProjectStatus::Offline,
                added_at: now,
                last_seen: now,
                total_routed: 0,
                total_errors: 0,
                description: description.to_string(),
            },
        );
    }

    pub fn remove_project(&self, name: &str) -> bool {
        self.projects.remove(name).is_some()
    }

    pub fn list_projects(&self) -> Vec<ProjectConnection> {
        self.projects
            .iter()
            .map(|e| e.value().clone())
            .collect()
    }

    pub fn get_project(&self, name: &str) -> Option<ProjectConnection> {
        self.projects.get(name).map(|e| e.value().clone())
    }

    // ===== Forwarding =====

    /// Forward a command to a specific project. Returns response or error.
    pub async fn forward_to_project(
        &self,
        project_name: &str,
        command_type: &str,
        payload: serde_json::Value,
    ) -> Result<BridgeResponse, String> {
        let project = self
            .projects
            .get(project_name)
            .ok_or_else(|| format!("project not found: {}", project_name))?
            .clone();

        let start = Instant::now();
        let cmd = BridgeCommand {
            source: "the-bridge-master".to_string(),
            command_type: command_type.to_string(),
            payload,
            timestamp_ns: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as i64,
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| format!("reqwest client: {}", e))?;

        let response = client
            .post(format!("{}/api/v1/bridge/receive", project.endpoint_url))
            .header("Authorization", format!("Bearer {}", project.auth_key_hmac))
            .json(&cmd)
            .send()
            .await
            .map_err(|e| format!("forward failed: {}", e))?;

        let duration_us = start.elapsed().as_micros() as u64;

        let result: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("response parse: {}", e))?;

        // Update project stats
        if let Some(mut p) = self.projects.get_mut(project_name) {
            p.total_routed += 1;
            p.last_seen = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            p.status = ProjectStatus::Online;
        }
        self.total_forwarded.fetch_add(1, Ordering::Relaxed);

        Ok(BridgeResponse {
            project: project_name.to_string(),
            status: "ok".to_string(),
            result,
            duration_us,
        })
    }

    /// Forward a trade/instruction to all projects with a specific capability.
    pub async fn broadcast(
        &self,
        command_type: &str,
        payload: serde_json::Value,
        capability: Option<Capability>,
    ) -> Vec<Result<BridgeResponse, String>> {
        let mut results = Vec::new();
        for entry in self.projects.iter() {
            let project = entry.value();
            if let Some(ref cap) = capability {
                if !project.capabilities.contains(cap) {
                    continue;
                }
            }
            let result = self.forward_to_project(&project.name, command_type, payload.clone()).await;
            results.push(result);
        }
        results
    }

    /// Forward a batch of trades to all projects that can receive orders.
    pub async fn forward_trades(
        &self,
        trades: &[crate::pipeline::TradePayload],
    ) -> Vec<Result<BridgeResponse, String>> {
        let payload = serde_json::json!({
            "trades": trades.iter().map(|t| serde_json::json!({
                "trade_id": t.trade_id,
                "pair": t.pair_str(),
                "price": t.price,
                "quantity": t.quantity,
                "total": t.total,
                "timestamp_ns": t.timestamp_ns,
            })).collect::<Vec<_>>(),
            "count": trades.len(),
        });
        self.broadcast("trade_settlement", payload, Some(Capability::ReceiveOrders))
            .await
    }

    // ===== Health =====

    pub fn project_count(&self) -> usize {
        self.projects.len()
    }

    pub fn mark_online(&self, name: &str) {
        if let Some(mut p) = self.projects.get_mut(name) {
            p.status = ProjectStatus::Online;
            p.last_seen = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
        }
    }

    pub fn mark_offline(&self, name: &str) {
        if let Some(mut p) = self.projects.get_mut(name) {
            p.status = ProjectStatus::Offline;
        }
    }

    pub fn total_forwarded(&self) -> u64 {
        self.total_forwarded.load(Ordering::Relaxed)
    }

    pub fn total_errors(&self) -> u64 {
        self.total_errors.load(Ordering::Relaxed)
    }

    // ===== Mesh Routing =====

    /// Forward a command through the Mesh P2P network instead of direct HTTP.
    /// Falls back to direct HTTP if mesh is unreachable.
    pub async fn forward_through_mesh(
        &self,
        project_name: &str,
        command_type: &str,
        payload: serde_json::Value,
    ) -> Result<BridgeResponse, String> {
        let _project = self
            .projects
            .get(project_name)
            .ok_or_else(|| format!("project not found: {}", project_name))?
            .clone();

        let mesh_url = std::env::var("THE_BRIDGE_MESH_URL")
            .unwrap_or_else(|_| "http://localhost:9777".to_string());

        let mesh_payload = serde_json::json!({
            "message_type": "BridgeForward",
            "target": project_name,
            "command_type": command_type,
            "payload": payload,
            "from": "the-bridge-master",
        });

        let start = Instant::now();
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| format!("reqwest client: {}", e))?;

        let response = client
            .post(format!("{}/api/v1/mesh/forward", mesh_url))
            .json(&mesh_payload)
            .send()
            .await
            .map_err(|e| format!("mesh forward failed: {}", e))?;

        let duration_us = start.elapsed().as_micros() as u64;
        let result: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("mesh response parse: {}", e))?;

        // Update project stats
        if let Some(mut p) = self.projects.get_mut(project_name) {
            p.total_routed += 1;
            p.last_seen = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            p.status = ProjectStatus::Online;
        }
        self.total_forwarded.fetch_add(1, Ordering::Relaxed);

        Ok(BridgeResponse {
            project: project_name.to_string(),
            status: "ok".to_string(),
            result,
            duration_us,
        })
    }

    /// Forward through mesh with automatic HTTP fallback.
    pub async fn route_to_project(
        &self,
        project_name: &str,
        command_type: &str,
        payload: serde_json::Value,
    ) -> Result<BridgeResponse, String> {
        // Try mesh first
        match self.forward_through_mesh(project_name, command_type, payload.clone()).await {
            Ok(resp) => Ok(resp),
            Err(_) => {
                // Fallback to direct HTTP
                self.forward_to_project(project_name, command_type, payload).await
            }
        }
    }

    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({
            "project_count": self.project_count(),
            "total_forwarded": self.total_forwarded(),
            "total_errors": self.total_errors(),
            "projects": self.list_projects().iter().map(|p| serde_json::json!({
                "name": p.name,
                "endpoint": p.endpoint_url,
                "status": p.status,
                "capabilities": p.capabilities,
                "total_routed": p.total_routed,
                "total_errors": p.total_errors,
                "description": p.description,
            })).collect::<Vec<_>>(),
        })
    }

    fn hash_key(&self, key: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}
