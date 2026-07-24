use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, warn};
use crate::types::{Order, Track};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DisclosureLevel {
    Public,
    Institutional,
    Government,
    Zero,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackConfig {
    pub track: Track,
    pub disclosure: DisclosureLevel,
    pub require_kyc: bool,
    pub iso20022_export: bool,
    pub audit_trail: bool,
    pub max_order_size: f64,
    pub settlement_network: String,
    pub fee_tier: &'static str,
    pub cross_track_allowed: bool,
}

impl Default for TrackConfig {
    fn default() -> Self {
        Self {
            track: Track::Compliant,
            disclosure: DisclosureLevel::Institutional,
            require_kyc: true,
            iso20022_export: true,
            audit_trail: true,
            max_order_size: 10_000_000.0,
            settlement_network: "dot".into(),
            fee_tier: "standard",
            cross_track_allowed: false,
        }
    }
}

impl TrackConfig {
    pub fn compliant() -> Self {
        Self {
            track: Track::Compliant,
            disclosure: DisclosureLevel::Institutional,
            require_kyc: true,
            iso20022_export: true,
            audit_trail: true,
            max_order_size: 10_000_000.0,
            settlement_network: "dot".into(),
            fee_tier: "standard",
            cross_track_allowed: false,
        }
    }

    pub fn ghost() -> Self {
        Self {
            track: Track::Autonomous,
            disclosure: DisclosureLevel::Zero,
            require_kyc: false,
            iso20022_export: false,
            audit_trail: false,
            max_order_size: 1_000_000.0,
            settlement_network: "zk".into(),
            fee_tier: "premium",
            cross_track_allowed: false,
        }
    }

    pub fn government() -> Self {
        Self {
            track: Track::Compliant,
            disclosure: DisclosureLevel::Government,
            require_kyc: true,
            iso20022_export: true,
            audit_trail: true,
            max_order_size: 100_000_000.0,
            settlement_network: "fedwire".into(),
            fee_tier: "wholesale",
            cross_track_allowed: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackedOrder {
    pub order: Order,
    pub disclosure: DisclosureLevel,
    pub phantom_id: Option<String>,
    pub routing_path: Vec<String>,
    pub zk_proof_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossTrackRule {
    pub compliant_can_match_ghost: bool,
    pub ghost_volume_limit_pct: f64,
    pub disclosure_on_match: DisclosureLevel,
    pub require_audit_on_ghost_match: bool,
}

impl Default for CrossTrackRule {
    fn default() -> Self {
        Self {
            compliant_can_match_ghost: false,
            ghost_volume_limit_pct: 10.0,
            disclosure_on_match: DisclosureLevel::Government,
            require_audit_on_ghost_match: true,
        }
    }
}

pub struct DualTrackRouter {
    configs: Vec<TrackConfig>,
    cross_rule: CrossTrackRule,
    routed_compliant: AtomicU64,
    routed_ghost: AtomicU64,
    cross_track_matches: AtomicU64,
    total_value_routed: AtomicU64,
}

impl DualTrackRouter {
    pub fn new() -> Self {
        let configs = vec![
            TrackConfig::compliant(),
            TrackConfig::ghost(),
            TrackConfig::government(),
        ];
        info!(
            "Dual Track Router initialized — {} tracks",
            configs.len(),
        );
        Self {
            configs,
            cross_rule: CrossTrackRule::default(),
            routed_compliant: AtomicU64::new(0),
            routed_ghost: AtomicU64::new(0),
            cross_track_matches: AtomicU64::new(0),
            total_value_routed: AtomicU64::new(0),
        }
    }

    pub fn route_order(&self, order: &Order) -> TrackedOrder {
        let config = self.config_for_track(&order.track);

        let _ = match order.track {
            Track::Compliant => self.routed_compliant.fetch_add(1, Ordering::Relaxed),
            Track::Autonomous => self.routed_ghost.fetch_add(1, Ordering::Relaxed),
        };
        self.total_value_routed.fetch_add(
            (order.price * order.quantity * 100.0) as u64,
            Ordering::Relaxed,
        );

        let phantom_id = if order.track == Track::Autonomous {
            let hash = Sha256::digest(order.id.to_string().as_bytes());
            Some(format!("phantom_{:x}", hash))
        } else {
            None
        };

        let tracked = TrackedOrder {
            order: order.clone(),
            disclosure: config.disclosure.clone(),
            phantom_id,
            routing_path: vec![config.settlement_network.clone()],
            zk_proof_required: order.track == Track::Autonomous,
        };

        if order.track == Track::Autonomous {
            info!(
                target: "ghost_track",
                order_id = %order.id,
                pair = %order.pair,
                value = order.price * order.quantity,
                phantom = tracked.phantom_id.as_deref().unwrap_or("none"),
                "Ghost Track: order routed with zero-disclosure"
            );
        }

        tracked
    }

    pub fn can_match(&self, maker: &Order, taker: &Order) -> (bool, DisclosureLevel) {
        if maker.track == taker.track {
            let config = self.config_for_track(&maker.track);
            return (true, config.disclosure.clone());
        }

        if !self.cross_rule.compliant_can_match_ghost {
            return (false, DisclosureLevel::Zero);
        }

        let ghost_volume = self.routed_ghost.load(Ordering::Relaxed) as f64;
        let total_volume = ghost_volume + self.routed_compliant.load(Ordering::Relaxed) as f64;
        if total_volume > 0.0 && (ghost_volume / total_volume * 100.0) > self.cross_rule.ghost_volume_limit_pct {
            return (false, DisclosureLevel::Zero);
        }

        self.cross_track_matches.fetch_add(1, Ordering::Relaxed);
        warn!(
            "Cross-track match: {:?} ↔ {:?} — disclosure forced to {:?}",
            maker.track, taker.track, self.cross_rule.disclosure_on_match
        );
        (true, self.cross_rule.disclosure_on_match.clone())
    }

    pub fn settlement_path(&self, tracked: &TrackedOrder) -> SettlementPath {
        match tracked.order.track {
            Track::Compliant => SettlementPath {
                network: "dot".into(),
                finality_ms: 16,
                requires_iso20022: true,
                requires_kyc: true,
                bridge_forward: true,
            },
            Track::Autonomous => SettlementPath {
                network: "zk-mesh".into(),
                finality_ms: 2000,
                requires_iso20022: false,
                requires_kyc: false,
                bridge_forward: tracked.disclosure != DisclosureLevel::Zero,
            },
        }
    }

    pub fn fee_for_track(&self, track: &Track, order_value: f64) -> f64 {
        match track {
            Track::Compliant => {
                if order_value > 1_000_000.0 { 0.001 }
                else if order_value > 100_000.0 { 0.002 }
                else { 0.003 }
            }
            Track::Autonomous => {
                if order_value > 100_000.0 { 0.005 }
                else { 0.008 }
            }
        }
    }

    pub fn disclosure_for_user(&self, user_tier: &str, track: &Track) -> DisclosureLevel {
        match (user_tier, track) {
            ("regulator", _) => DisclosureLevel::Government,
            ("institution", Track::Compliant) => DisclosureLevel::Institutional,
            ("institution", Track::Autonomous) => DisclosureLevel::Zero,
            ("retail", Track::Compliant) => DisclosureLevel::Public,
            ("retail", Track::Autonomous) => DisclosureLevel::Zero,
            _ => DisclosureLevel::Public,
        }
    }

    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({
            "tracks": self.configs.iter().map(|c| serde_json::json!({
                "track": c.track,
                "disclosure": c.disclosure,
                "require_kyc": c.require_kyc,
                "iso20022": c.iso20022_export,
                "settlement": c.settlement_network,
                "max_order": c.max_order_size,
            })).collect::<Vec<_>>(),
            "stats": {
                "routed_compliant": self.routed_compliant.load(Ordering::Relaxed),
                "routed_ghost": self.routed_ghost.load(Ordering::Relaxed),
                "cross_track_matches": self.cross_track_matches.load(Ordering::Relaxed),
                "total_value_routed": self.total_value_routed.load(Ordering::Relaxed),
            },
            "cross_track_rule": {
                "allowed": self.cross_rule.compliant_can_match_ghost,
                "ghost_volume_limit_pct": self.cross_rule.ghost_volume_limit_pct,
            },
        })
    }

    pub fn get_track_config(&self, track: &Track) -> Option<&TrackConfig> {
        self.configs.iter().find(|c| c.track == *track)
    }

    fn config_for_track(&self, track: &Track) -> &TrackConfig {
        self.configs.iter().find(|c| c.track == *track)
            .unwrap_or(&self.configs[0])
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementPath {
    pub network: String,
    pub finality_ms: u64,
    pub requires_iso20022: bool,
    pub requires_kyc: bool,
    pub bridge_forward: bool,
}

pub fn validate_track_switch(order: &Order, new_track: Track) -> Result<TrackedOrder, String> {
    if order.track == new_track {
        return Err("order already on this track".into());
    }
    if order.filled > 0.0 {
        return Err("cannot switch track on partially filled order".into());
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    if now - order.timestamp > 300 {
        return Err("cannot switch track on orders older than 5 minutes".into());
    }
    Ok(TrackedOrder {
        order: order.clone(),
        disclosure: DisclosureLevel::Zero,
        phantom_id: None,
        routing_path: vec![],
        zk_proof_required: false,
    })
}
