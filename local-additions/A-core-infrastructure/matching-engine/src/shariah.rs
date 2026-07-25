use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use crate::types::Order;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShariahAuditEntry {
    pub order_id: String,
    pub user_id: String,
    pub pair: String,
    pub approved: bool,
    pub reason: String,
    pub timestamp: i64,
}

pub struct ShariahFilter {
    pub enabled: bool,
    prohibited_pairs: HashSet<String>,
    prohibited_industries: Vec<&'static str>,
    audit_trail: Vec<ShariahAuditEntry>,
    max_audit: usize,
}

impl ShariahFilter {
    pub fn new(enabled: bool) -> Self {
        let mut prohibited = HashSet::new();
        for symbol in PROHIBITED_SYMBOLS {
            prohibited.insert(symbol.to_string());
        }

        Self {
            enabled,
            prohibited_pairs: prohibited,
            prohibited_industries: PROHIBITED_INDUSTRIES.to_vec(),
            audit_trail: Vec::with_capacity(1000),
            max_audit: 10_000,
        }
    }

    pub fn check_order(&mut self, order: &Order) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }

        let pair = order.pair.as_str();
        let base = pair.split('/').next().unwrap_or(pair).trim();

        if self.prohibited_pairs.contains(base) || self.prohibited_pairs.contains(pair) {
            self.audit(order, false, format!("Prohibited pair: {}", pair));
            return Err(format!("Shariah: pair {} is prohibited", pair));
        }

        let industries: Vec<&str> = self.prohibited_industries.clone();
        for industry in &industries {
            if pair.to_uppercase().contains(&industry.to_uppercase()) ||
               base.to_uppercase().contains(&industry.to_uppercase()) {
                self.audit(order, false, format!("Prohibited industry: {} in pair {}", industry, pair));
                return Err(format!("Shariah: pair {} relates to prohibited industry ({})", pair, industry));
            }
        }

        if order.is_swap {
            self.audit(order, false, "SWAP orders are not Shariah-compliant".into());
            return Err("Shariah: SWAP/derivative orders are not permitted".into());
        }

        self.audit(order, true, "Approved".into());
        Ok(())
    }

    fn audit(&mut self, order: &Order, approved: bool, reason: String) {
        if self.audit_trail.len() >= self.max_audit {
            self.audit_trail.remove(0);
        }
        self.audit_trail.push(ShariahAuditEntry {
            order_id: order.id.to_string(),
            user_id: order.user_id.to_string(),
            pair: order.pair.to_string(),
            approved,
            reason,
            timestamp: chrono::Utc::now().timestamp_millis(),
        });
    }

    pub fn add_prohibited_pair(&mut self, pair: &str) {
        self.prohibited_pairs.insert(pair.to_uppercase());
    }

    pub fn recent_audit(&self, n: usize) -> Vec<ShariahAuditEntry> {
        let count = n.min(self.audit_trail.len());
        self.audit_trail.iter().rev().take(count).cloned().collect()
    }

    pub fn audit_count(&self) -> (usize, usize) {
        let approved = self.audit_trail.iter().filter(|e| e.approved).count();
        let rejected = self.audit_trail.len() - approved;
        (approved, rejected)
    }
}

const PROHIBITED_SYMBOLS: &[&str] = &[
    "WIN", "LOTT", "CASINO", "GAMBLING", "GOLD-DERIVATIVE",
];

const PROHIBITED_INDUSTRIES: &[&str] = &[
    "GAMBLING", "CASINO", "ALCOHOL", "PORK", "WEAPONS",
    "DEFENSE", "TOBACCO", "ADULT", "RIBA",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use uuid::Uuid;

    #[test]
    fn test_approves_spot() {
        let mut filter = ShariahFilter::new(true);
        let order = Order::new_limit(Uuid::new_v4(), "BTC/USD".into(), OrderSide::Buy, 50000.0, 1.0);
        assert!(filter.check_order(&order).is_ok());
    }

    #[test]
    fn test_rejects_gambling() {
        let mut filter = ShariahFilter::new(true);
        let order = Order::new_limit(Uuid::new_v4(), "WIN/USD".into(), OrderSide::Buy, 10.0, 100.0);
        assert!(filter.check_order(&order).is_err());
    }

    #[test]
    fn test_rejects_swap() {
        let mut filter = ShariahFilter::new(true);
        let order = Order::new_swap(Uuid::new_v4(), "BTC".into(), "USD".into(), 1.0);
        assert!(filter.check_order(&order).is_err());
    }

    #[test]
    fn test_disabled_bypass() {
        let mut filter = ShariahFilter::new(false);
        let order = Order::new_limit(Uuid::new_v4(), "WIN/USD".into(), OrderSide::Buy, 10.0, 100.0);
        assert!(filter.check_order(&order).is_ok());
    }

    #[test]
    fn test_audit_trail() {
        let mut filter = ShariahFilter::new(true);
        let order = Order::new_limit(Uuid::new_v4(), "BTC/USD".into(), OrderSide::Buy, 50000.0, 1.0);
        let _ = filter.check_order(&order);
        let bad = Order::new_limit(Uuid::new_v4(), "WIN/USD".into(), OrderSide::Buy, 10.0, 100.0);
        let _ = filter.check_order(&bad);
        let (approved, rejected) = filter.audit_count();
        assert_eq!(approved, 1);
        assert_eq!(rejected, 1);
    }
}
