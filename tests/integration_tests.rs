//! THE-BRIDGE Integration Tests
//! Tests for all 6 crates: core, flash-loan, arbitrage, mev-protection, chaos, integration

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;
    use uuid::Uuid;

    // ============ CORE CRATE TESTS ============
    #[test]
    fn test_core_types_token() {
        let token = the_bridge_core::types::Token {
            address: [1u8; 20],
            symbol: "ETH".into(),
            decimals: 18,
        };
        assert_eq!(token.symbol, "ETH");
        assert_eq!(token.decimals, 18);
    }

    #[test]
    fn test_core_types_price() {
        let price = the_bridge_core::types::Price::new(1000_000_000, 8);
        assert_eq!(price.value, 1000_000_000);
        assert_eq!(price.decimals, 8);
    }

    #[test]
    fn test_core_flash_loan_provider_names() {
        use the_bridge_core::types::FlashLoanProvider;
        assert_eq!(FlashLoanProvider::AaveV3.name(), "aave-v3");
        assert_eq!(FlashLoanProvider::UniswapV3.name(), "uniswap-v3");
    }

    #[test]
    fn test_core_type_sizes() {
        use std::mem::size_of;
        assert_eq!(size_of::<the_bridge_core::types::OrderSide>(), 1);
        assert_eq!(size_of::<the_bridge_core::types::OrderStatus>(), 1);
    }

    // ============ FLASH LOAN CRATE TESTS ============
    #[test]
    fn test_flash_loan_error_display() {
        use the_bridge_flash_loan::*;
        let e = FlashLoanError::InsufficientLiquidity { token: [0u8;20], requested: 0, available: 0 };
        let s = format!("{}", e);
        assert!(s.contains("InsufficientLiquidity"));
    }

    #[test]
    fn test_flash_loan_result_success() {
        use the_bridge_flash_loan::*;
        let result = FlashLoanResult::success([0u8; 32], 100_000, 1000, 1000, 1000, "aave".into());
        assert!(result.success);
        assert_eq!(result.gas_used, 100_000);
    }

    #[test]
    fn test_flash_loan_result_failure() {
        use the_bridge_flash_loan::*;
        let result = FlashLoanResult::failure("mock", "gas estimation failed");
        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[test]
    fn test_flash_loan_callback() {
        use the_bridge_flash_loan::*;
        let cb = FlashLoanCallback {
            data: vec![1, 2, 3],
            gas_limit: 500_000,
            assets: vec![[0u8; 20]],
            amounts: vec![1000_000],
        };
        assert_eq!(cb.data.len(), 3);
        assert_eq!(cb.gas_limit, 500_000);
    }

    #[test]
    fn test_flash_loan_pool() {
        use the_bridge_flash_loan::*;
        let pool = FlashLoanPool {
            address: [1u8; 20],
            token: [2u8; 20],
            available: 1000_000,
            total: 10_000_000,
            fee: 9,
        };
        assert_eq!(pool.fee, 9);
    }

    #[test]
    fn test_flash_loan_router_new() {
        use the_bridge_flash_loan::*;
        use std::sync::Arc;
        let router = FlashLoanRouter::new(vec![]);
        assert!(router.providers.is_empty());
    }

    #[test]
    fn test_flash_loan_mock_provider() {
        use the_bridge_flash_loan::*;
        use std::sync::Arc;
        let provider = MockProvider::new("mock-aave", 9, vec![[0u8; 20]]);
        assert_eq!(provider.name(), "mock-aave");
    }

    #[test]
    fn test_flash_loan_aave_constants() {
        use the_bridge_flash_loan::*;
        assert_eq!(AAVE_V3_POOL, "0x87870Bca3F3fD6335C3F4ce8392D69350B4fA4E2");
    }

    #[test]
    fn test_flash_loan_uniswap_constants() {
        use the_bridge_flash_loan::*;
        assert_eq!(UNISWAP_V3_FACTORY, "0x1F98431c8aD98523631AE4a59f267346ea31F984");
    }

    // ============ ARBITRAGE CRATE TESTS ============
    #[test]
    fn test_arbitrage_tick_math_constants() {
        use the_bridge_arbitrage::*;
        assert!(MIN_TICK < MAX_TICK);
        assert!(MIN_SQRT_RATIO < MAX_SQRT_RATIO);
    }

    #[test]
    fn test_arbitrage_tick_math_round_trip() {
        use the_bridge_arbitrage::*;
        let tick = 5000;
        let sqrt = get_sqrt_ratio_at_tick(tick).unwrap();
        let tick_back = get_tick_at_sqrt_ratio(sqrt).unwrap();
        assert_eq!(tick, tick_back);
    }

    #[test]
    fn test_arbitrage_tick_math_min_max() {
        use the_bridge_arbitrage::*;
        let sqrt_min = get_sqrt_ratio_at_tick(MIN_TICK).unwrap();
        let sqrt_max = get_sqrt_ratio_at_tick(MAX_TICK).unwrap();
        assert!(sqrt_min < sqrt_max);
    }

    #[test]
    fn test_arbitrage_pool_get_amount_out() {
        use the_bridge_arbitrage::*;
        let pool = PoolData {
            address: [0u8; 20],
            token_a: [1u8; 20],
            token_b: [2u8; 20],
            fee: 3000,
            liquidity: 100_000,
            sqrt_price: 1u128 << 64,
            tick: 0,
        };
        let amount_out = pool.get_amount_out(1000, [1u8; 20]).unwrap();
        assert!(amount_out > 0);
    }

    #[test]
    fn test_arbitrage_path_finder_new() {
        use the_bridge_arbitrage::*;
        let finder = PathFinder::new(vec![]);
        let paths = finder.find_all_paths([0u8; 20], [1u8; 20], 3, 5);
        assert!(paths.is_empty());
    }

    #[test]
    fn test_arbitrage_profit_optimizer() {
        use the_bridge_arbitrage::*;
        let opt = ProfitOptimizer { gas_price: 50_000_000_000u128 };
        let gas_cost = opt.calculate_gas_cost(100_000);
        assert!(gas_cost > 0);
    }

    #[test]
    fn test_arbitrage_kelly_criterion() {
        use the_bridge_arbitrage::*;
        let kelly = ProfitOptimizer::kelly_criterion(0.6, 2.0);
        assert!((kelly - 0.3).abs() < 0.01);
    }

    #[test]
    fn test_arbitrage_engine_new() {
        use the_bridge_arbitrage::*;
        let finder = PathFinder::new(vec![]);
        let engine = ArbitrageEngine::new(finder, ProfitOptimizer { gas_price: 50_000_000_000 }, 10);
        assert_eq!(engine.min_profit_bps, 10);
    }

    // ============ MEV PROTECTION CRATE TESTS ============
    #[test]
    fn test_mev_error_display() {
        use the_bridge_mev_protection::*;
        let e = MevError::BundleRejected("bad bundle".into());
        assert!(format!("{}", e).contains("bad bundle"));
    }

    #[test]
    fn test_mev_bundle_builder_basic() {
        use the_bridge_mev_protection::*;
        let mut builder = MevBundleBuilder::new();
        builder.add_transaction(vec![1, 2, 3]);
        builder.set_block_range(100, 200);
        builder.set_priority_fee(50_000_000_000u128);
        let bundle = builder.build().unwrap();
        assert_eq!(bundle.txs.len(), 1);
        assert_eq!(bundle.block_number, 100);
        assert_eq!(bundle.max_block, 200);
    }

    #[test]
    fn test_mev_bundle_builder_invalid() {
        use the_bridge_mev_protection::*;
        let mut builder = MevBundleBuilder::new();
        builder.set_block_range(200, 100);
        assert!(builder.build().is_err());
    }

    #[test]
    fn test_mev_bundle_id() {
        use the_bridge_mev_protection::*;
        let mut builder = MevBundleBuilder::new();
        builder.add_transaction(vec![1, 2, 3]);
        builder.set_block_range(1, 10);
        let bundle = builder.build().unwrap();
        assert_eq!(bundle.id.get_version(), Some(uuid::Version::Random));
    }

    #[test]
    fn test_mev_flashbots_relay_new() {
        use the_bridge_mev_protection::*;
        let relay = FlashbotsRelayClient::new("https://relay.flashbots.net", [0u8; 32]);
        assert_eq!(relay.relay_url, "https://relay.flashbots.net");
    }

    #[test]
    fn test_mev_share_client_new() {
        use the_bridge_mev_protection::*;
        let share = MevShareClient::new("https://mev-share.flashbots.net", [0u8; 32]);
        assert_eq!(share.relay_url, "https://mev-share.flashbots.net");
    }

    #[test]
    fn test_mev_fee_estimator_default() {
        use the_bridge_mev_protection::*;
        let estimator = PriorityFeeEstimator::new(50);
        assert_eq!(estimator.percentile, 50);
    }

    #[test]
    fn test_mev_fee_estimator_adjust() {
        use the_bridge_mev_protection::*;
        let mut estimator = PriorityFeeEstimator::new(50);
        estimator.adjust_percentile(90);
        assert_eq!(estimator.percentile, 90);
    }

    #[test]
    fn test_mev_tip_strategy() {
        use the_bridge_mev_protection::*;
        match TipStrategy::Conservative {
            TipStrategy::Conservative => {},
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_mev_format_gwei() {
        use the_bridge_mev_protection::*;
        let s = format_gwei(1_000_000_000u128);
        assert_eq!(s, "1.000000000 gwei");
    }

    // ============ CHAOS CRATE TESTS ============
    #[tokio::test]
    async fn test_chaos_poison_basic() {
        use the_bridge_chaos::*;
        let oracle = OraclePricePoisoning::new([1u8; 32], 1000, 100);
        assert!(!oracle.is_poisoned());
        oracle.poison(None).await.unwrap();
        assert!(oracle.is_poisoned());
    }

    #[tokio::test]
    async fn test_chaos_poison_restore() {
        use the_bridge_chaos::*;
        let oracle = OraclePricePoisoning::new([1u8; 32], 500, 200);
        oracle.poison(None).await.unwrap();
        assert!(oracle.is_poisoned());
        oracle.restore().await.unwrap();
        assert!(!oracle.is_poisoned());
    }

    #[tokio::test]
    async fn test_chaos_detect_poisoning() {
        use the_bridge_chaos::*;
        let now = chrono::Utc::now().timestamp() as u64;
        let feeds = vec![
            PriceFeedSnapshot { feed_id: [2u8; 32], price: 1000, timestamp: now - 10, source: "chainlink".into(), block_number: 100 },
            PriceFeedSnapshot { feed_id: [2u8; 32], price: 2000, timestamp: now, source: "chainlink".into(), block_number: 101 },
        ];
        assert!(OraclePricePoisoning::detect(&feeds, 50).await.unwrap());
    }

    #[tokio::test]
    async fn test_chaos_recovery_sign() {
        use the_bridge_chaos::*;
        let r = UnilateralRecovery::new([3u8; 20], 1, Duration::from_secs(60));
        let signed = r.sign(RecoveryAction::FreezeAll, &[4u8; 32]).await.unwrap();
        assert!(r.verify(&signed).unwrap());
    }

    #[tokio::test]
    async fn test_chaos_timelock() {
        use the_bridge_chaos::*;
        let tl = TimeLockedTransaction::new();
        let id = tl.schedule([5u8; 20], vec![100], chrono::Utc::now().timestamp() as u64 + 3600).await.unwrap();
        assert!(!tl.pending().await.is_empty());
    }

    #[tokio::test]
    async fn test_chaos_experiment() {
        use the_bridge_chaos::*;
        let o = std::sync::Arc::new(OraclePricePoisoning::new([6u8; 32], 1000, 50));
        let mut p = HashMap::new();
        p.insert("duration_secs".into(), "1".into());
        p.insert("deviation_bps".into(), "100".into());
        let mut exp = ChaosExperiment::new("test", ChaosType::OraclePoisoning, p, Some(o));
        let r = exp.run().await.unwrap();
        assert!(r.ok);
    }

    #[tokio::test]
    async fn test_chaos_engine() {
        use the_bridge_chaos::*;
        let engine = ChaosEngine::new(
            OraclePricePoisoning::new([7u8; 32], 1000, 50),
            UnilateralRecovery::new([8u8; 20], 1, Duration::from_secs(60)),
            TimeLockedTransaction::new(),
        );
        let mut p = HashMap::new();
        p.insert("duration_secs".into(), "1".into());
        assert!(engine.run_exp("test", ChaosType::OraclePoisoning, p).await.unwrap().ok);
    }

    #[tokio::test]
    async fn test_chaos_health() {
        use the_bridge_chaos::*;
        let engine = ChaosEngine::new(
            OraclePricePoisoning::new([9u8; 32], 1000, 50),
            UnilateralRecovery::new([10u8; 20], 1, Duration::from_secs(60)),
            TimeLockedTransaction::new(),
        );
        let h = engine.health().await;
        assert!(h.ok);
    }

    // ============ INTEGRATION CRATE TESTS ============
    #[test]
    fn test_integration_event_bus() {
        use the_bridge_integration::*;
        let bus = EventBus::new("test", 100);
        let mut rx = bus.subscribe();
        bus.publish(SystemEvent::ComponentStarted { name: "engine".into(), instance_id: Uuid::new_v4() }).unwrap();
        assert!(rx.try_recv().is_ok());
    }

    #[test]
    fn test_integration_health() {
        use the_bridge_integration::*;
        let h = HealthAggregator::new(Duration::from_secs(5), 3);
        h.update(HealthReport {
            component: "engine".into(), instance_id: Uuid::new_v4(), healthy: true,
            last_heartbeat: 0, uptime: Duration::from_secs(100), error_count: 0,
            latency_p50: Duration::from_secs(0), latency_p90: Duration::from_secs(0),
            latency_p99: Duration::from_secs(0), memory_used_mb: 0, cpu_usage_pct: 0.0,
        });
        assert!(h.is_healthy());
    }

    #[test]
    fn test_integration_metrics() {
        use the_bridge_integration::*;
        let m = MetricsCollector::new("test");
        m.gauge("tps", 5000.0);
        assert_eq!(m.get_gauge("tps"), Some(5000.0));
        m.inc("orders", 10);
        assert_eq!(m.get_counter("orders"), 10);
    }

    #[test]
    fn test_integration_config() {
        use the_bridge_integration::*;
        let c = ConfigManager::new();
        c.set("max_tps", "1000000").unwrap();
        assert_eq!(c.get("max_tps").unwrap(), "1000000");
        assert!(c.has("max_tps"));
    }

    #[test]
    fn test_integration_mesh() {
        use the_bridge_integration::*;
        let m = ServiceMesh::new();
        let inst = m.register("engine", "1.0.0", vec!["http://localhost:3001".into()]);
        assert_eq!(m.discover("engine").unwrap().len(), 1);
        m.deregister("engine", inst.id).unwrap();
        assert!(m.discover("engine").is_err());
    }

    #[test]
    fn test_integration_engine() {
        use the_bridge_integration::*;
        struct TestComp { name: String }
        #[async_trait::async_trait]
        impl ComponentLifecycle for TestComp {
            fn name(&self) -> &str { &self.name }
            fn metadata(&self) -> ComponentMetadata {
                ComponentMetadata { name: self.name.clone(), version: "1".into(), description: "".into(), dependencies: vec![], capabilities: vec!["test".into()] }
            }
            async fn start(&self) -> Result<()> { Ok(()) }
            async fn stop(&self) -> Result<()> { Ok(()) }
            async fn health_check(&self) -> Result<HealthReport> {
                Ok(HealthReport { component: self.name.clone(), instance_id: Uuid::new_v4(), healthy: true, last_heartbeat: 0, uptime: Duration::from_secs(1), error_count: 0, latency_p50: Duration::from_secs(0), latency_p90: Duration::from_secs(0), latency_p99: Duration::from_secs(0), memory_used_mb: 0, cpu_usage_pct: 0.0 })
            }
        }
        let engine = IntegrationEngine::new();
        let comp = std::sync::Arc::new(TestComp { name: "test".into() });
        engine.register(comp).unwrap();
        assert_eq!(engine.component_count(), 1);
    }

    #[test]
    fn test_integration_config_watch() {
        use the_bridge_integration::*;
        let c = ConfigManager::new();
        let mut rx = c.watch();
        c.set("key", "val").unwrap();
        let (k, v) = rx.try_recv().unwrap();
        assert_eq!(k, "key");
        assert_eq!(v, "val");
    }
}
