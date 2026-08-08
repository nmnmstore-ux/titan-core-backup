use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::ghost_integration::BrokerEndpoint;
use crate::types::{OrderSide, Track};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteRequest {
    pub user_id: String,
    pub pair: String,
    pub side: OrderSide,
    pub quantity: f64,
    pub price: f64,
    pub track: Track,
    pub max_slippage_bps: u64,
    pub prefer_latency: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerRoute {
    pub broker_id: String,
    pub quantity: f64,
    pub estimated_cost: f64,
    pub estimated_latency_us: u64,
    pub reliability_score: f64,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteResult {
    pub request_id: String,
    pub routes: Vec<BrokerRoute>,
    pub total_quantity: f64,
    pub estimated_total_cost: f64,
    pub best_latency_us: u64,
    pub worst_latency_us: u64,
    pub strategy: String,
}

pub struct SmartOrderRouter {
    broker_history: HashMap<String, BrokerHistory>,
    latency_penalty_weight: f64,
    cost_weight: f64,
    reliability_weight: f64,
    history_decay: f64,
}

#[derive(Debug, Clone)]
struct BrokerHistory {
    success_count: u64,
    fail_count: u64,
    avg_latency_us: u64,
    last_seen: i64,
}

impl SmartOrderRouter {
    pub fn new() -> Self {
        Self {
            broker_history: HashMap::new(),
            latency_penalty_weight: 0.3,
            cost_weight: 0.4,
            reliability_weight: 0.3,
            history_decay: 0.95,
        }
    }

    pub fn with_weights(latency: f64, cost: f64, reliability: f64) -> Self {
        let total = latency + cost + reliability;
        Self {
            broker_history: HashMap::new(),
            latency_penalty_weight: latency / total,
            cost_weight: cost / total,
            reliability_weight: reliability / total,
            history_decay: 0.95,
        }
    }

    fn score_broker(
        &self,
        broker: &BrokerEndpoint,
        request: &RouteRequest,
    ) -> f64 {
        let cost_score = self.cost_score(broker, request);
        let latency_score = self.latency_score(broker);
        let reliability_score = self.reliability_score(&broker.id);

        cost_score * self.cost_weight
            + latency_score * self.latency_penalty_weight
            + reliability_score * self.reliability_weight
    }

    fn cost_score(&self, broker: &BrokerEndpoint, request: &RouteRequest) -> f64 {
        let base_fee = match request.track {
            Track::Compliant => 0.001,
            Track::Autonomous => 0.003,
        };
        let broker_fee = base_fee * (1.0 + (1.0 - broker.weight));
        let slippage_cost = (request.max_slippage_bps as f64) / 10000.0;
        let total = broker_fee + slippage_cost;
        (1.0 - total).max(0.0)
    }

    fn latency_score(&self, broker: &BrokerEndpoint) -> f64 {
        let latency_us = broker.latency_base_us;
        if latency_us == 0 { return 1.0; }
        let score = 1.0 - (latency_us as f64 / 1_000_000.0).min(1.0);
        score
    }

    fn reliability_score(&self, broker_id: &str) -> f64 {
        match self.broker_history.get(broker_id) {
            Some(h) => {
                let total = h.success_count + h.fail_count;
                if total == 0 { return 0.9; }
                let base = h.success_count as f64 / total as f64;
                let decay = self.history_decay.powi(total as i32);
                base * decay + 0.9 * (1.0 - decay)
            }
            None => 0.9,
        }
    }

    pub fn route(
        &mut self,
        request: &RouteRequest,
        available_brokers: &[BrokerEndpoint],
    ) -> RouteResult {
        let mut scored: Vec<(f64, &BrokerEndpoint)> = available_brokers
            .iter()
            .filter(|b| b.is_active && request.quantity <= b.max_order_size)
            .map(|b| (self.score_broker(b, request), b))
            .collect();
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        let request_id = format!(
            "route_{:x}",
            Sha256::digest(format!("{}{}{:?}", request.user_id, request.pair, request.quantity))
        );

        if scored.is_empty() {
            return RouteResult {
                request_id,
                routes: vec![],
                total_quantity: 0.0,
                estimated_total_cost: 0.0,
                best_latency_us: 0,
                worst_latency_us: 0,
                strategy: "no_route".into(),
            };
        }

        let mut routes = Vec::new();
        let mut remaining = request.quantity;

        let total_value = request.quantity * request.price;
        if scored.len() == 1 || total_value < 5000.0 {
            let (score, broker) = &scored[0];
            let cost = request.quantity * request.price * 0.001;
            routes.push(BrokerRoute {
                broker_id: broker.id.clone(),
                quantity: remaining,
                estimated_cost: cost,
                estimated_latency_us: broker.latency_base_us,
                reliability_score: self.reliability_score(&broker.id),
                score: *score,
            });
        } else {
            let total_weight: f64 = scored.iter().map(|(s, _)| s).sum();
            let _rng = rand::thread_rng();
            for (i, (score, broker)) in scored.iter().enumerate() {
                if i == scored.len() - 1 {
                    let cost = remaining * request.price * 0.001;
                    routes.push(BrokerRoute {
                        broker_id: broker.id.clone(),
                        quantity: remaining,
                        estimated_cost: cost,
                        estimated_latency_us: broker.latency_base_us,
                        reliability_score: self.reliability_score(&broker.id),
                        score: *score,
                    });
                    break;
                }
                let allocation = (request.quantity * score / total_weight)
                    .min(broker.max_order_size)
                    .min(remaining);
                if allocation < 1.0 { continue; }
                let cost = allocation * request.price * 0.001;
                routes.push(BrokerRoute {
                    broker_id: broker.id.clone(),
                    quantity: allocation,
                    estimated_cost: cost,
                    estimated_latency_us: broker.latency_base_us,
                    reliability_score: self.reliability_score(&broker.id),
                    score: *score,
                });
                remaining -= allocation;
                if remaining < 1.0 { break; }
            }
        }

        let total_cost: f64 = routes.iter().map(|r| r.estimated_cost).sum();
        let best_lat = routes.iter().map(|r| r.estimated_latency_us).min().unwrap_or(0);
        let worst_lat = routes.iter().map(|r| r.estimated_latency_us).max().unwrap_or(0);
        let total_qty: f64 = routes.iter().map(|r| r.quantity).sum();

        RouteResult {
            request_id,
            routes,
            total_quantity: total_qty,
            estimated_total_cost: total_cost,
            best_latency_us: best_lat,
            worst_latency_us: worst_lat,
            strategy: if scored.len() == 1 { "single".into() } else { "split_weighted".into() },
        }
    }

    pub fn record_success(&mut self, broker_id: &str, latency_us: u64) {
        let entry = self.broker_history.entry(broker_id.to_string()).or_insert(BrokerHistory {
            success_count: 0, fail_count: 0, avg_latency_us: 0, last_seen: 0,
        });
        entry.success_count += 1;
        entry.avg_latency_us = (entry.avg_latency_us * (entry.success_count - 1) + latency_us) / entry.success_count;
        entry.last_seen = chrono::Utc::now().timestamp();
    }

    pub fn record_failure(&mut self, broker_id: &str) {
        let entry = self.broker_history.entry(broker_id.to_string()).or_insert(BrokerHistory {
            success_count: 0, fail_count: 0, avg_latency_us: 0, last_seen: 0,
        });
        entry.fail_count += 1;
        entry.last_seen = chrono::Utc::now().timestamp();
    }

    pub fn snapshot(&self) -> serde_json::Value {
        let history: serde_json::Value = self.broker_history.iter().map(|(id, h)| {
            (id.clone(), serde_json::json!({
                "success_rate": if h.success_count + h.fail_count > 0 {
                    format!("{:.2}%", h.success_count as f64 / (h.success_count + h.fail_count) as f64 * 100.0)
                } else { "N/A".to_string() },
                "avg_latency_us": h.avg_latency_us,
                "total_attempts": h.success_count + h.fail_count,
                "last_seen": h.last_seen,
            }))
        }).collect();
        serde_json::json!({
            "weights": {
                "latency": self.latency_penalty_weight,
                "cost": self.cost_weight,
                "reliability": self.reliability_weight,
            },
            "history": history,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::OrderSide;

    fn make_broker(id: &str, weight: f64, max_size: f64, latency: u64) -> BrokerEndpoint {
        BrokerEndpoint {
            id: id.into(),
            name: format!("Broker {}", id),
            url: format!("http://{}.local", id),
            weight,
            max_order_size: max_size,
            latency_base_us: latency,
            is_active: true,
            total_routed: 0,
            last_used: 0,
        }
    }

    #[test]
    fn test_single_broker_route() {
        let mut router = SmartOrderRouter::new();
        let brokers = vec![make_broker("b1", 1.0, 1_000_000.0, 100)];
        let req = RouteRequest {
            user_id: "u1".into(), pair: "USD/EGP".into(),
            side: OrderSide::Buy, quantity: 1000.0, price: 30.50,
            track: Track::Compliant, max_slippage_bps: 10,
            prefer_latency: false,
        };
        let result = router.route(&req, &brokers);
        assert_eq!(result.routes.len(), 1);
        assert!((result.total_quantity - 1000.0).abs() < 0.01);
        assert_eq!(result.strategy, "single");
    }

    #[test]
    fn test_multi_broker_split() {
        let mut router = SmartOrderRouter::new();
        let brokers = vec![
            make_broker("fast", 0.8, 500_000.0, 50),
            make_broker("cheap", 1.0, 1_000_000.0, 200),
            make_broker("big", 0.6, 5_000_000.0, 150),
        ];
        let req = RouteRequest {
            user_id: "u2".into(), pair: "USD/EGP".into(),
            side: OrderSide::Buy, quantity: 100_000.0, price: 30.50,
            track: Track::Compliant, max_slippage_bps: 10,
            prefer_latency: false,
        };
        let result = router.route(&req, &brokers);
        assert!(result.routes.len() > 1, "should split large order, got {} routes", result.routes.len());
        assert!((result.total_quantity - 100_000.0).abs() < 1.0);
        assert_eq!(result.strategy, "split_weighted");
        assert!(result.best_latency_us <= result.worst_latency_us);
    }

    #[test]
    fn test_ghost_track_route() {
        let mut router = SmartOrderRouter::new();
        let brokers = vec![make_broker("ghost-broker", 0.9, 500_000.0, 80)];
        let req = RouteRequest {
            user_id: "anon_1".into(), pair: "BTC/USD".into(),
            side: OrderSide::Buy, quantity: 1.5, price: 65000.0,
            track: Track::Autonomous, max_slippage_bps: 50,
            prefer_latency: false,
        };
        let result = router.route(&req, &brokers);
        assert_eq!(result.routes.len(), 1);
        assert_eq!(result.strategy, "single");
    }

    #[test]
    fn test_no_active_brokers() {
        let mut router = SmartOrderRouter::new();
        let mut broker = make_broker("offline", 1.0, 1000.0, 100);
        broker.is_active = false;
        let brokers = vec![broker];
        let req = RouteRequest {
            user_id: "u3".into(), pair: "USD/EGP".into(),
            side: OrderSide::Sell, quantity: 500.0, price: 30.50,
            track: Track::Compliant, max_slippage_bps: 10,
            prefer_latency: false,
        };
        let result = router.route(&req, &brokers);
        assert!(result.routes.is_empty());
        assert_eq!(result.total_quantity, 0.0);
        assert_eq!(result.strategy, "no_route");
    }

    #[test]
    fn test_record_success_failure() {
        let mut router = SmartOrderRouter::new();
        router.record_success("broker-a", 120);
        router.record_success("broker-a", 80);
        router.record_failure("broker-a");

        let snap = router.snapshot();
        let hist = snap["history"]["broker-a"].as_object().unwrap();
        assert_eq!(hist["total_attempts"], 3);
        assert!(hist["avg_latency_us"].as_u64().unwrap() > 0);
    }

    #[test]
    fn test_large_order_over_max() {
        let mut router = SmartOrderRouter::new();
        let brokers = vec![make_broker("small", 1.0, 10_000.0, 50)];
        let req = RouteRequest {
            user_id: "u4".into(), pair: "USD/EGP".into(),
            side: OrderSide::Buy, quantity: 100_000.0, price: 30.50,
            track: Track::Compliant, max_slippage_bps: 10,
            prefer_latency: false,
        };
        let result = router.route(&req, &brokers);
        assert!(result.routes.is_empty(), "should reject order > broker max");
    }
}
