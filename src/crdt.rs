#![allow(dead_code)]
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

type NodeId = String;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VersionVector {
    pub clocks: HashMap<NodeId, u64>,
}

impl VersionVector {
    pub fn new() -> Self {
        Self { clocks: HashMap::new() }
    }

    pub fn increment(&mut self, node: &str) {
        *self.clocks.entry(node.to_string()).or_insert(0) += 1;
    }

    pub fn get(&self, node: &str) -> u64 {
        self.clocks.get(node).copied().unwrap_or(0)
    }

    pub fn descends_from(&self, other: &VersionVector) -> bool {
        for (node, clock) in &other.clocks {
            if self.clocks.get(node).copied().unwrap_or(0) < *clock {
                return false;
            }
        }
        true
    }

    pub fn merge(&mut self, other: &VersionVector) {
        for (node, clock) in &other.clocks {
            let entry = self.clocks.entry(node.clone()).or_insert(0);
            *entry = (*entry).max(*clock);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CRDTOrder {
    pub order: crate::types::Order,
    pub version: VersionVector,
    pub tombstone: bool,
}

pub struct CRDTReplica {
    node_id: String,
    orders: DashMap<uuid::Uuid, CRDTOrder>,
}

impl CRDTReplica {
    pub fn new(node_id: &str) -> Self {
        Self {
            node_id: node_id.to_string(),
            orders: DashMap::new(),
        }
    }

    pub fn apply_add(&self, order: crate::types::Order, source: &str) {
        let mut vv = VersionVector::new();
        vv.increment(source);

        self.orders.entry(order.id).and_modify(|existing| {
            if vv.descends_from(&existing.version) {
                existing.order = order.clone();
                existing.version.merge(&vv);
            }
        }).or_insert(CRDTOrder {
            order,
            version: vv,
            tombstone: false,
        });
    }

    pub fn apply_remove(&self, id: uuid::Uuid, source: &str) {
        let mut vv = VersionVector::new();
        vv.increment(source);

        if let Some(mut entry) = self.orders.get_mut(&id) {
            if vv.descends_from(&entry.version) {
                entry.tombstone = true;
                entry.version.merge(&vv);
            }
        } else {
            self.orders.insert(id, CRDTOrder {
                order: crate::types::Order {
                    id,
                    ..Default::default()
                },
                version: vv,
                tombstone: true,
            });
        }
    }

    pub fn merge(&self, remote: &CRDTReplica) {
        for item in remote.orders.iter() {
            let id = item.key();
            let remote_order = item.value();

            self.orders.entry(*id).and_modify(|local| {
                if remote_order.version.descends_from(&local.version) {
                    local.order = remote_order.order.clone();
                    local.version = remote_order.version.clone();
                    local.tombstone = remote_order.tombstone;
                } else if !local.version.descends_from(&remote_order.version) {
                    local.version.merge(&remote_order.version);
                }
            }).or_insert_with(|| remote_order.clone());
        }
    }

    pub fn active_orders(&self) -> Vec<crate::types::Order> {
        self.orders.iter()
            .filter(|e| !e.tombstone)
            .map(|e| e.order.clone())
            .collect()
    }

    pub fn get_order(&self, id: &uuid::Uuid) -> Option<crate::types::Order> {
        self.orders.get(id).filter(|e| !e.tombstone).map(|e| e.order.clone())
    }

    pub fn snapshot(&self) -> Vec<(uuid::Uuid, CRDTOrder)> {
        self.orders.iter().map(|e| (*e.key(), e.value().clone())).collect()
    }

    pub fn load_snapshot(&self, data: Vec<(uuid::Uuid, CRDTOrder)>) {
        for (id, order) in data {
            self.orders.insert(id, order);
        }
    }
}
