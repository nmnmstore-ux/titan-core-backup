//! Multi-RPC fallback / rotation with health scoring.
//!
//! Public endpoints rate-limit aggressively (429) and can hang. This module
//! tracks a set of endpoint URLs, scores them by reliability (consecutive
//! failures and 429s), and rotates across them. It prefers provider-class
//! endpoints (Alchemy / Ankr / QuickNode) but falls back to public nodes.

use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// After this many consecutive failures (or a throttle/5xx), cool the
/// endpoint down for `COOLDOWN_SECS`.
const FAIL_THRESHOLD: u32 = 2;
const COOLDOWN_SECS: u64 = 30;

struct EndpointState {
    consecutive_failures: u32,
    total_requests: u64,
    total_failures: u64,
    cooldown_until: Option<Instant>,
}

impl Default for EndpointState {
    fn default() -> Self {
        Self {
            consecutive_failures: 0,
            total_requests: 0,
            total_failures: 0,
            cooldown_until: None,
        }
    }
}

fn in_cooldown(st: &EndpointState) -> bool {
    st.cooldown_until.map(|c| Instant::now() < c).unwrap_or(false)
}

/// Ordered, de-duplicated set of RPC endpoints with health tracking.
pub struct RpcPool {
    pub endpoints: Vec<String>,
    states: Mutex<Vec<EndpointState>>,
    cursor: AtomicUsize,
    client: reqwest::Client,
}

impl RpcPool {
    pub fn new(endpoints: Vec<String>) -> Self {
        let mut seen = HashSet::new();
        let mut unique: Vec<String> = Vec::new();
        for e in endpoints {
            let t = e.trim().to_string();
            if t.is_empty() {
                continue;
            }
            if seen.insert(t.clone()) {
                unique.push(t);
            }
        }
        if unique.is_empty() {
            unique.push("https://ethereum-rpc.publicnode.com".to_string());
        }

        let states: Vec<EndpointState> = (0..unique.len()).map(|_| EndpointState::default()).collect();

        Self {
            endpoints: unique,
            states: Mutex::new(states),
            cursor: AtomicUsize::new(0),
            client: reqwest::Client::builder()
                .pool_max_idle_per_host(16)
                .pool_idle_timeout(Duration::from_secs(8))
                .timeout(Duration::from_secs(8))
                .build()
                .unwrap_or_default(),
        }
    }

    fn normalize(&self, url: &str) -> String {
        url.split('?').next().unwrap_or(url).to_string()
    }

    fn index_of(&self, url: &str) -> Option<usize> {
        let clean = self.normalize(url);
        self.endpoints
            .iter()
            .position(|e| self.normalize(e) == clean)
    }

    /// Next endpoint to try: round-robin across non-cooldown endpoints. If all
    /// are cooling down, falls back to the first to avoid a permanent deadlock.
    pub fn next_healthy(&self) -> String {
        let n = self.endpoints.len();
        let states = self.states.lock().unwrap();
        let start = self.cursor.fetch_add(1, Ordering::Relaxed);
        for i in 0..n {
            let idx = (start + i) % n;
            if !in_cooldown(&states[idx]) {
                return self.normalize(&self.endpoints[idx]);
            }
        }
        self.normalize(&self.endpoints[0])
    }

    pub fn report_success(&self, url: &str) {
        if let Some(idx) = self.index_of(url) {
            let mut states = self.states.lock().unwrap();
            let st = &mut states[idx];
            st.consecutive_failures = 0;
            st.total_requests += 1;
        }
    }

    /// Record a failure. `throttled` should be true on HTTP 429 / 5xx / network
    /// errors, in which case the endpoint goes straight into cooldown.
    pub fn report_failure(&self, url: &str, throttled: bool) {
        if let Some(idx) = self.index_of(url) {
            let mut states = self.states.lock().unwrap();
            let st = &mut states[idx];
            st.total_failures += 1;
            st.total_requests += 1;
            st.consecutive_failures += 1;
            let should_cool = throttled || st.consecutive_failures >= FAIL_THRESHOLD;
            if should_cool {
                st.cooldown_until = Some(Instant::now() + Duration::from_secs(COOLDOWN_SECS));
                st.consecutive_failures = 0;
            }
        }
    }

    /// Performs a JSON-RPC request, transparently rotating across healthy
    /// endpoints until one answers or all have been tried.
    ///
    /// Returns `(parsed_response, endpoint_used)`. A JSON-RPC response that is
    /// a well-formed error object still counts as a successful exchange with
    /// that node (the node is alive); callers inspect `"result"`/`"error"`.
    pub async fn rpc_call(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<(serde_json::Value, String), String> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": 1,
        });

        let max_attempts = (self.endpoints.len() as u32).max(2);
        let mut last_err: Option<String> = None;

        for _ in 0..max_attempts {
            let url = self.next_healthy();

            match self.client.post(&url).json(&body).send().await {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        match resp.json::<serde_json::Value>().await {
                            Ok(json) => {
                                self.report_success(&url);
                                return Ok((json, url));
                            }
                            Err(e) => {
                                self.report_failure(&url, true);
                                last_err = Some(format!("invalid json: {}", e));
                            }
                        }
                    } else {
                        let throttled = status == reqwest::StatusCode::TOO_MANY_REQUESTS
                            || status.as_u16() >= 500;
                        self.report_failure(&url, throttled);
                        last_err = Some(format!("http {}", status.as_u16()));
                    }
                }
                Err(e) => {
                    self.report_failure(&url, true);
                    last_err = Some(format!("network: {}", e));
                }
            }
        }

        Err(last_err.unwrap_or_else(|| "all rpc endpoints failed".to_string()))
    }

    pub fn len(&self) -> usize {
        self.endpoints.len()
    }

    fn next_url(&self) -> String {
        let u = self.next_healthy();
        self.normalize(&u)
    }
}

/// Convenience: build a pool from the standard environment variables. The
/// primary RPC comes first, then `RPC_FALLBACK_ENDPOINTS` (comma separated),
/// then public nodes as a last resort.
pub fn pool_from_env(rpc_url: &str) -> RpcPool {
    let mut list = vec![rpc_url.to_string()];

    if let Ok(extra) = std::env::var("RPC_FALLBACK_ENDPOINTS") {
        for ep in extra.split(',') {
            let ep = ep.trim().to_string();
            if !ep.is_empty() {
                list.push(ep);
            }
        }
    }

    for public in [
        "https://ethereum-rpc.publicnode.com",
        "https://1rpc.io/eth",
        "https://eth.drpc.org",
    ] {
        list.push(public.to_string());
    }

    RpcPool::new(list)
}