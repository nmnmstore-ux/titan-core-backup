use std::sync::Arc;
use std::time::Duration;

#[path = "../src/cloak.rs"]
mod cloak;
#[path = "../src/snapshot.rs"]
mod snapshot;

use cloak::{SovereignKillSwitch, NodeCloakingProtocol, CloakSignal, ThreatAnalyzer, ThreatLevel};
use snapshot::EngineSnapshot;

#[test]
fn test_threat_analyzer_green() {
    let analyzer = ThreatAnalyzer::new();
    assert_eq!(analyzer.analyze(), ThreatLevel::Green);
}

#[test]
fn test_threat_analyzer_rate_limit_triggers_orange() {
    let analyzer = ThreatAnalyzer::new();
    for _ in 0..15_000 {
        analyzer.record_request("1.2.3.4".into(), "/api/v1/order".into());
    }
    let level = analyzer.analyze();
    assert!(level == ThreatLevel::Orange || level == ThreatLevel::Red);
}

#[test]
fn test_kill_switch_activate() {
    let ks = SovereignKillSwitch::new(vec!["10.0.0.1:3001".into()]);
    assert!(!ks.is_cloaked());
    let level = ks.activate();
    assert_eq!(level, ThreatLevel::Black);
    assert!(ks.is_cloaked());
    assert_eq!(ks.total_cloaks.load(std::sync::atomic::Ordering::Relaxed), 1);
}

#[test]
fn test_cloak_signal_creation() {
    let mut fiat = std::collections::HashMap::new();
    fiat.insert("USDC".into(), 1_000_000.0);
    fiat.insert("EGP".into(), 15_000_000.0);

    let signal = CloakSignal::new("engine-1", ThreatLevel::Red, fiat);
    assert_eq!(signal.node_id, "engine-1");
    assert_eq!(signal.threat_level, ThreatLevel::Red);
    assert_eq!(signal.convert_to_rwa_gold.len(), 2);
    assert!(signal.convert_to_rwa_gold.iter().all(|fb| fb.target_gold_contract == "0xRWA_GOLD_TOKEN"));
}

#[test]
fn test_hot_migration_timeout() {
    let result = NodeCloakingProtocol::execute_hot_migration(&[], vec![], vec![]);
    assert!(result.is_ok());
}

#[test]
fn test_snapshot_creation() {
    let snap = NodeCloakingProtocol::create_snapshot(vec![], vec![]);
    assert_eq!(snap.version, env!("CARGO_PKG_VERSION"));
    assert!(snap.timestamp > 0);
    assert_eq!(snap.tee_attestation, "sgx-enclave-verified");
}
