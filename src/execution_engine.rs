use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionConfig {
    pub enable_anti_mev: bool,
    pub enable_smart_routing: bool,
    pub enable_internalization: bool,
    pub max_slippage_bps: u32,
    pub mev_protection_bps: u32,
    pub routing_timeout_ms: u64,
    pub partial_fill_enabled: bool,
    pub min_fill_size_usd: f64,
    pub price_improvement_target_bps: u32,
    pub venues: Vec<ExecutionVenue>,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            enable_anti_mev: true,
            enable_smart_routing: true,
            enable_internalization: true,
            max_slippage_bps: 20,
            mev_protection_bps: 5,
            routing_timeout_ms: 100,
            partial_fill_enabled: true,
            min_fill_size_usd: 100.0,
            price_improvement_target_bps: 2,
            venues: vec![
                ExecutionVenue {
                    venue_id: "INTERNAL".to_string(),
                    venue_type: ExecutionVenueType::Internal,
                    fee_bps: 1,
                    latency_ms: 1,
                    max_order_size_usd: Some(10_000_000.0),
                    supported_order_types: vec![OrderType::Market, OrderType::Limit, OrderType::Iceberg],
                    enabled: true,
                    priority: 1,
                },
                ExecutionVenue {
                    venue_id: "BINANCE".to_string(),
                    venue_type: ExecutionVenueType::ExternalCEX,
                    fee_bps: 10,
                    latency_ms: 50,
                    max_order_size_usd: Some(5_000_000.0),
                    supported_order_types: vec![OrderType::Market, OrderType::Limit],
                    enabled: true,
                    priority: 2,
                },
                ExecutionVenue {
                    venue_id: "UNISWAP_V3".to_string(),
                    venue_type: ExecutionVenueType::ExternalDEX,
                    fee_bps: 30,
                    latency_ms: 500,
                    max_order_size_usd: Some(1_000_000.0),
                    supported_order_types: vec![OrderType::Market, OrderType::Limit],
                    enabled: true,
                    priority: 3,
                },
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionVenue {
    pub venue_id: String,
    pub venue_type: ExecutionVenueType,
    pub fee_bps: u32,
    pub latency_ms: u32,
    pub max_order_size_usd: Option<f64>,
    pub supported_order_types: Vec<OrderType>,
    pub enabled: bool,
    pub priority: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExecutionVenueType {
    Internal,
    ExternalCEX,
    ExternalDEX,
    OTC,
    DarkPool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum OrderType {
    Market,
    Limit,
    Iceberg,
    TWAP,
    VWAP,
    Pegged,
    Stop,
    StopLimit,
    TrailingStop,
    Conditional,
    MultiLeg,
    CrossAsset,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OrderStatus {
    New,
    Pending,
    PartiallyFilled,
    Filled,
    Cancelled,
    Rejected,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvancedOrder {
    pub order_id: String,
    pub client_order_id: String,
    pub participant_id: String,
    pub symbol: String,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub quantity: f64,
    pub filled_quantity: f64,
    pub remaining_quantity: f64,
    pub price: Option<f64>,
    pub stop_price: Option<f64>,
    pub limit_price: Option<f64>,
    pub trailing_offset: Option<f64>,
    pub time_in_force: TimeInForce,
    pub status: OrderStatus,
    pub created_at: u64,
    pub updated_at: u64,
    pub expires_at: Option<u64>,
    pub execution_instructions: ExecutionInstructions,
    pub algo_params: AlgoParams,
    pub legs: Vec<OrderLeg>,
    pub parent_order_id: Option<String>,
    pub child_orders: Vec<String>,
    pub tags: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TimeInForce {
    GTC,
    IOC,
    FOK,
    GTD,
    AtTheClose,
    AtTheOpen,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExecutionInstructions {
    pub allow_partial_fill: bool,
    pub min_fill_size_usd: f64,
    pub max_slippage_bps: u32,
    pub prefer_venue: Option<String>,
    pub avoid_venues: Vec<String>,
    pub enable_price_improvement: bool,
    pub anti_mev: bool,
    pub post_only: bool,
    pub reduce_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AlgoParams {
    pub twap_duration_sec: Option<u64>,
    pub twap_interval_sec: Option<u64>,
    pub vwap_start_time: Option<u64>,
    pub vwap_end_time: Option<u64>,
    pub pegged_offset_bps: Option<i32>,
    pub pegged_reference: Option<PegReference>,
    pub iceberg_display_qty: Option<f64>,
    pub iceberg_refresh_threshold: Option<f64>,
    pub stop_trigger: Option<StopTrigger>,
    pub trailing_activation_price: Option<f64>,
    pub condition: Option<ExecutionCondition>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PegReference {
    MidPrice,
    BestBid,
    BestAsk,
    LastTrade,
    VWAP,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StopTrigger {
    LastPrice,
    MarkPrice,
    IndexPrice,
    BidPrice,
    AskPrice,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionCondition {
    pub condition_type: ConditionType,
    pub symbol: String,
    pub operator: ComparisonOperator,
    pub value: f64,
    pub time_window_sec: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConditionType {
    PriceAbove,
    PriceBelow,
    VolumeAbove,
    VolatilityBelow,
    SpreadBelow,
    TimeAfter,
    TimeBefore,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ComparisonOperator {
    GreaterThan,
    LessThan,
    GreaterThanOrEqual,
    LessThanOrEqual,
    Equal,
    NotEqual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderLeg {
    pub leg_id: String,
    pub symbol: String,
    pub side: OrderSide,
    pub ratio: f64,
    pub order_type: OrderType,
    pub price: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionReport {
    pub execution_id: String,
    pub order_id: String,
    pub client_order_id: String,
    pub participant_id: String,
    pub symbol: String,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub price: f64,
    pub quantity: f64,
    pub filled_quantity: f64,
    pub remaining_quantity: f64,
    pub status: OrderStatus,
    pub venue: String,
    pub fee_usd: f64,
    pub fee_asset: String,
    pub timestamp: u64,
    pub liquidity_flag: LiquidityFlag,
    pub price_improvement_bps: f64,
    pub slippage_bps: f64,
    pub mev_protected: bool,
    pub child_order_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LiquidityFlag {
    Maker,
    Taker,
    Internalized,
    Crossed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartRouteResult {
    pub route_id: String,
    pub order_id: String,
    pub legs: Vec<RouteLeg>,
    pub estimated_fill_price: f64,
    pub estimated_slippage_bps: f64,
    pub estimated_fees_usd: f64,
    pub estimated_latency_ms: u64,
    pub mev_risk_score: f64,
    pub confidence: f64,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteLeg {
    pub venue_id: String,
    pub venue_type: ExecutionVenueType,
    pub side: OrderSide,
    pub quantity: f64,
    pub limit_price: f64,
    pub expected_fill_rate: f64,
    pub fee_bps: u32,
    pub latency_ms: u32,
    pub priority: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExecutionMetrics {
    pub total_orders: u64,
    pub filled_orders: u64,
    pub cancelled_orders: u64,
    pub rejected_orders: u64,
    pub total_volume_usd: f64,
    pub avg_fill_rate_pct: f64,
    pub avg_slippage_bps: f64,
    pub avg_price_improvement_bps: f64,
    pub mev_attacks_prevented: u64,
    pub internalization_rate_pct: f64,
    pub avg_latency_ms: f64,
    pub venue_distribution: HashMap<String, f64>,
    pub order_type_distribution: HashMap<OrderType, f64>,
}

pub struct ExecutionEngine {
    config: ExecutionConfig,
    orders: Arc<RwLock<HashMap<String, AdvancedOrder>>>,
    execution_reports: Arc<RwLock<Vec<ExecutionReport>>>,
    pending_routes: Arc<RwLock<HashMap<String, SmartRouteResult>>>,
    active_algos: Arc<RwLock<HashMap<String, AlgoExecutionState>>>,
    metrics: Arc<RwLock<ExecutionMetrics>>,
    mev_detector: Arc<RwLock<MEVDetector>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlgoExecutionState {
    pub order_id: String,
    pub algo_type: OrderType,
    pub next_execution_at: u64,
    pub remaining_quantity: f64,
    pub executed_quantity: f64,
    pub avg_price: f64,
    pub params: AlgoParams,
    pub status: AlgoStatus,
    pub child_orders: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AlgoStatus {
    Running,
    Paused,
    Completed,
    Cancelled,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MEVDetector {
    pub pending_txs: HashMap<String, PendingTx>,
    pub attack_patterns: Vec<AttackPattern>,
    pub detected_attacks: u64,
    pub prevented_attacks: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingTx {
    pub tx_hash: String,
    pub from: String,
    pub to: String,
    pub value: f64,
    pub gas_price: u64,
    pub timestamp: u64,
    pub decoded_data: Option<DecodedSwap>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecodedSwap {
    pub dex: String,
    pub token_in: String,
    pub token_out: String,
    pub amount_in: f64,
    pub min_amount_out: f64,
    pub path: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttackPattern {
    pub pattern_id: String,
    pub name: String,
    pub description: String,
    pub indicators: Vec<String>,
    pub severity: f64,
    pub enabled: bool,
}

impl ExecutionEngine {
    pub fn new(config: ExecutionConfig) -> Self {
        let mev_detector = MEVDetector {
            pending_txs: HashMap::new(),
            attack_patterns: vec![
                AttackPattern {
                    pattern_id: "sandwich".to_string(),
                    name: "Sandwich Attack".to_string(),
                    description: "Buy before victim, sell after".to_string(),
                    indicators: vec!["large_buy".to_string(), "immediate_sell".to_string(), "same_block".to_string()],
                    severity: 0.9,
                    enabled: true,
                },
                AttackPattern {
                    pattern_id: "frontrun".to_string(),
                    name: "Frontrunning".to_string(),
                    description: "Execute before known large order".to_string(),
                    indicators: vec!["high_gas".to_string(), "known_pool".to_string(), "pre_execution".to_string()],
                    severity: 0.8,
                    enabled: true,
                },
                AttackPattern {
                    pattern_id: "backrun".to_string(),
                    name: "Backrunning".to_string(),
                    description: "Execute after known transaction".to_string(),
                    indicators: vec!["known_result".to_string(), "arbitrage".to_string()],
                    severity: 0.6,
                    enabled: true,
                },
            ],
            detected_attacks: 0,
            prevented_attacks: 0,
        };

        Self {
            config,
            orders: Arc::new(RwLock::new(HashMap::new())),
            execution_reports: Arc::new(RwLock::new(Vec::new())),
            pending_routes: Arc::new(RwLock::new(HashMap::new())),
            active_algos: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(RwLock::new(ExecutionMetrics::default())),
            mev_detector: Arc::new(RwLock::new(mev_detector)),
        }
    }

    pub async fn submit_order(&self, order: AdvancedOrder) -> Result<ExecutionReport, String> {
        let mut orders = self.orders.write().await;
        if orders.contains_key(&order.order_id) {
            return Err("Order ID already exists".to_string());
        }

        let validated = self.validate_order(&order).await?;
        orders.insert(order.order_id.clone(), validated.clone());

        let report = if self.config.enable_smart_routing && matches!(validated.order_type, OrderType::Market | OrderType::Limit) {
            self.execute_smart_route(&validated).await?
        } else if self.is_algo_order(&validated.order_type) {
            self.start_algo_execution(&validated).await?
        } else {
            self.execute_direct(&validated).await?
        };

        self.update_metrics(&report).await;
        Ok(report)
    }

    async fn validate_order(&self, order: &AdvancedOrder) -> Result<AdvancedOrder, String> {
        if order.quantity <= 0.0 {
            return Err("Invalid quantity".to_string());
        }
        if matches!(order.order_type, OrderType::Limit) && order.price.is_none() {
            return Err("Limit price required for limit orders".to_string());
        }
        if matches!(order.order_type, OrderType::Stop | OrderType::StopLimit) && order.stop_price.is_none() {
            return Err("Stop price required for stop orders".to_string());
        }
        if order.execution_instructions.min_fill_size_usd > 0.0 && order.quantity * order.price.unwrap_or(0.0) < order.execution_instructions.min_fill_size_usd {
            return Err("Order size below minimum".to_string());
        }
        
        let mut validated = order.clone();
        validated.remaining_quantity = order.quantity;
        validated.filled_quantity = 0.0;
        validated.status = OrderStatus::New;
        validated.created_at = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        validated.updated_at = validated.created_at;
        
        Ok(validated)
    }

    fn is_algo_order(&self, order_type: &OrderType) -> bool {
        matches!(order_type, OrderType::TWAP | OrderType::VWAP | OrderType::Pegged | OrderType::Iceberg | OrderType::TrailingStop | OrderType::Conditional)
    }

    async fn execute_smart_route(&self, order: &AdvancedOrder) -> Result<ExecutionReport, String> {
        let route = self.calculate_smart_route(order).await?;
        
        let mut total_filled = 0.0;
        let mut total_cost = 0.0;
        let mut total_fees = 0.0;
        let mut venues_used = Vec::new();
        let mut child_orders = Vec::new();
        
        for leg in &route.legs {
            let fill_result = self.execute_leg(order, leg).await?;
            total_filled += fill_result.filled_quantity;
            total_cost += fill_result.filled_quantity * fill_result.price;
            total_fees += fill_result.fee_usd;
            venues_used.push(leg.venue_id.clone());
            child_orders.extend(fill_result.child_order_ids);
        }

        let avg_price = if total_filled > 0.0 { total_cost / total_filled } else { 0.0 };
        let slippage_bps = self.calculate_slippage(order, avg_price);
        let price_improvement = self.calculate_price_improvement(order, avg_price);
        
        let report = ExecutionReport {
            execution_id: Uuid::new_v4().to_string(),
            order_id: order.order_id.clone(),
            client_order_id: order.client_order_id.clone(),
            participant_id: order.participant_id.clone(),
            symbol: order.symbol.clone(),
            side: order.side.clone(),
            order_type: order.order_type.clone(),
            price: avg_price,
            quantity: order.quantity,
            filled_quantity: total_filled,
            remaining_quantity: order.quantity - total_filled,
            status: if total_filled >= order.quantity { OrderStatus::Filled } else { OrderStatus::PartiallyFilled },
            venue: "SMART_ROUTER".to_string(),
            fee_usd: total_fees,
            fee_asset: "USD".to_string(),
            timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
            liquidity_flag: if total_filled >= order.quantity { LiquidityFlag::Taker } else { LiquidityFlag::Taker },
            price_improvement_bps: price_improvement,
            slippage_bps,
            mev_protected: self.config.enable_anti_mev,
            child_order_ids: child_orders,
        };

        self.execution_reports.write().await.push(report.clone());
        
        let mut orders = self.orders.write().await;
        if let Some(o) = orders.get_mut(&order.order_id) {
            o.filled_quantity = total_filled;
            o.remaining_quantity = order.quantity - total_filled;
            o.status = report.status;
            o.updated_at = report.timestamp;
        }

        Ok(report)
    }

    async fn calculate_smart_route(&self, order: &AdvancedOrder) -> Result<SmartRouteResult, String> {
        let notional = order.quantity * order.price.unwrap_or(0.0);
        let mut legs = Vec::new();
        let mut remaining = order.quantity;
        
        let sorted_venues = {
            let mut v = self.config.venues.clone();
            v.sort_by_key(|v| v.priority);
            v
        };
        
        for venue in sorted_venues {
            if !venue.enabled || remaining <= 0.0 { continue; }
            if !venue.supported_order_types.contains(&order.order_type) { continue; }
            if let Some(max_size) = venue.max_order_size_usd {
                if notional > max_size { continue; }
            }
            
            let leg_qty = remaining.min(notional / venue.max_order_size_usd.unwrap_or(f64::MAX));
            if leg_qty <= 0.0 { continue; }
            
            legs.push(RouteLeg {
                venue_id: venue.venue_id.clone(),
                venue_type: venue.venue_type.clone(),
                side: order.side.clone(),
                quantity: leg_qty,
                limit_price: order.price.unwrap_or(0.0),
                expected_fill_rate: 0.95,
                fee_bps: venue.fee_bps,
                latency_ms: venue.latency_ms,
                priority: venue.priority,
            });
            
            remaining -= leg_qty;
        }
        
        let estimated_fill_price = order.price.unwrap_or(0.0);
        let estimated_slippage_bps = 2.0;
        let estimated_fees_usd = legs.iter().map(|l| l.quantity * l.limit_price * l.fee_bps as f64 / 10000.0).sum();
        let estimated_latency_ms = legs.iter().map(|l| l.latency_ms as u64).max().unwrap_or(0);
        let mev_risk_score = if self.config.enable_anti_mev { 0.1 } else { 0.5 };
        
        Ok(SmartRouteResult {
            route_id: Uuid::new_v4().to_string(),
            order_id: order.order_id.clone(),
            legs,
            estimated_fill_price,
            estimated_slippage_bps,
            estimated_fees_usd,
            estimated_latency_ms,
            mev_risk_score,
            confidence: 0.9,
            created_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
        })
    }

    async fn execute_leg(&self, order: &AdvancedOrder, leg: &RouteLeg) -> Result<ExecutionReport, String> {
        let filled = leg.quantity * leg.expected_fill_rate;
        let fee = filled * leg.limit_price * leg.fee_bps as f64 / 10000.0;
        
        let child_id = Uuid::new_v4().to_string();
        
        let report = ExecutionReport {
            execution_id: Uuid::new_v4().to_string(),
            order_id: order.order_id.clone(),
            client_order_id: format!("{}_{}", order.client_order_id, child_id),
            participant_id: order.participant_id.clone(),
            symbol: order.symbol.clone(),
            side: order.side.clone(),
            order_type: order.order_type.clone(),
            price: leg.limit_price,
            quantity: leg.quantity,
            filled_quantity: filled,
            remaining_quantity: leg.quantity - filled,
            status: if filled >= leg.quantity { OrderStatus::Filled } else { OrderStatus::PartiallyFilled },
            venue: leg.venue_id.clone(),
            fee_usd: fee,
            fee_asset: "USD".to_string(),
            timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
            liquidity_flag: LiquidityFlag::Taker,
            price_improvement_bps: self.config.price_improvement_target_bps as f64,
            slippage_bps: 1.0,
            mev_protected: self.config.enable_anti_mev,
            child_order_ids: vec![child_id],
        };
        
        Ok(report)
    }

    async fn execute_direct(&self, order: &AdvancedOrder) -> Result<ExecutionReport, String> {
        let venue = self.config.venues.first().cloned().ok_or("No venues configured")?;
        let filled = order.quantity;
        let price = order.price.unwrap_or(0.0);
        let fee = filled * price * venue.fee_bps as f64 / 10000.0;
        
        let report = ExecutionReport {
            execution_id: Uuid::new_v4().to_string(),
            order_id: order.order_id.clone(),
            client_order_id: order.client_order_id.clone(),
            participant_id: order.participant_id.clone(),
            symbol: order.symbol.clone(),
            side: order.side.clone(),
            order_type: order.order_type.clone(),
            price,
            quantity: order.quantity,
            filled_quantity: filled,
            remaining_quantity: 0.0,
            status: OrderStatus::Filled,
            venue: venue.venue_id,
            fee_usd: fee,
            fee_asset: "USD".to_string(),
            timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
            liquidity_flag: LiquidityFlag::Internalized,
            price_improvement_bps: self.config.price_improvement_target_bps as f64,
            slippage_bps: 0.0,
            mev_protected: self.config.enable_anti_mev,
            child_order_ids: vec![],
        };
        
        self.execution_reports.write().await.push(report.clone());
        Ok(report)
    }

    async fn start_algo_execution(&self, order: &AdvancedOrder) -> Result<ExecutionReport, String> {
        let state = AlgoExecutionState {
            order_id: order.order_id.clone(),
            algo_type: order.order_type.clone(),
            next_execution_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
            remaining_quantity: order.quantity,
            executed_quantity: 0.0,
            avg_price: 0.0,
            params: order.algo_params.clone(),
            status: AlgoStatus::Running,
            child_orders: vec![],
        };
        
        self.active_algos.write().await.insert(order.order_id.clone(), state);
        
        let report = ExecutionReport {
            execution_id: Uuid::new_v4().to_string(),
            order_id: order.order_id.clone(),
            client_order_id: order.client_order_id.clone(),
            participant_id: order.participant_id.clone(),
            symbol: order.symbol.clone(),
            side: order.side.clone(),
            order_type: order.order_type.clone(),
            price: order.price.unwrap_or(0.0),
            quantity: order.quantity,
            filled_quantity: 0.0,
            remaining_quantity: order.quantity,
            status: OrderStatus::Pending,
            venue: "ALGO_ENGINE".to_string(),
            fee_usd: 0.0,
            fee_asset: "USD".to_string(),
            timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
            liquidity_flag: LiquidityFlag::Maker,
            price_improvement_bps: 0.0,
            slippage_bps: 0.0,
            mev_protected: self.config.enable_anti_mev,
            child_order_ids: vec![],
        };
        
        self.execution_reports.write().await.push(report.clone());
        Ok(report)
    }

    pub async fn cancel_order(&self, order_id: &str) -> Result<(), String> {
        let mut orders = self.orders.write().await;
        if let Some(order) = orders.get_mut(order_id) {
            order.status = OrderStatus::Cancelled;
            order.updated_at = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
            
            self.active_algos.write().await.remove(order_id);
            
            let report = ExecutionReport {
                execution_id: Uuid::new_v4().to_string(),
                order_id: order_id.to_string(),
                client_order_id: order.client_order_id.clone(),
                participant_id: order.participant_id.clone(),
                symbol: order.symbol.clone(),
                side: order.side.clone(),
                order_type: order.order_type.clone(),
                price: order.price.unwrap_or(0.0),
                quantity: order.quantity,
                filled_quantity: order.filled_quantity,
                remaining_quantity: order.remaining_quantity,
                status: OrderStatus::Cancelled,
                venue: "CANCELLED".to_string(),
                fee_usd: 0.0,
                fee_asset: "USD".to_string(),
                timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
                liquidity_flag: LiquidityFlag::Maker,
                price_improvement_bps: 0.0,
                slippage_bps: 0.0,
                mev_protected: false,
                child_order_ids: vec![],
            };
            
            self.execution_reports.write().await.push(report);
            Ok(())
        } else {
            Err("Order not found".to_string())
        }
    }

    fn calculate_slippage(&self, order: &AdvancedOrder, fill_price: f64) -> f64 {
        let reference = order.price.unwrap_or(fill_price);
        if reference == 0.0 { return 0.0; }
        let slippage = (fill_price - reference) / reference;
        match order.side {
            OrderSide::Buy => slippage * 10000.0,
            OrderSide::Sell => -slippage * 10000.0,
        }
    }

    fn calculate_price_improvement(&self, order: &AdvancedOrder, fill_price: f64) -> f64 {
        let reference = order.price.unwrap_or(fill_price);
        if reference == 0.0 { return 0.0; }
        let improvement = (reference - fill_price) / reference;
        match order.side {
            OrderSide::Buy => improvement * 10000.0,
            OrderSide::Sell => -improvement * 10000.0,
        }
    }

    async fn update_metrics(&self, report: &ExecutionReport) {
        let mut metrics = self.metrics.write().await;
        metrics.total_orders += 1;
        if report.status == OrderStatus::Filled {
            metrics.filled_orders += 1;
        } else if report.status == OrderStatus::Cancelled {
            metrics.cancelled_orders += 1;
        } else if report.status == OrderStatus::Rejected {
            metrics.rejected_orders += 1;
        }
        
        metrics.total_volume_usd += report.filled_quantity * report.price;
        
        let alpha = 0.1;
        metrics.avg_slippage_bps = metrics.avg_slippage_bps * (1.0 - alpha) + report.slippage_bps.abs() * alpha;
        metrics.avg_price_improvement_bps = metrics.avg_price_improvement_bps * (1.0 - alpha) + report.price_improvement_bps * alpha;
        
        *metrics.venue_distribution.entry(report.venue.clone()).or_insert(0.0) += report.filled_quantity * report.price;
        *metrics.order_type_distribution.entry(report.order_type.clone()).or_insert(0.0) += 1.0;
        
        if report.mev_protected {
            metrics.mev_attacks_prevented += 1;
        }
    }

    pub async fn detect_mev(&self, tx: PendingTx) -> Result<Vec<AttackPattern>, String> {
        let detector = self.mev_detector.read().await;
        let mut detected = Vec::new();
        
        for pattern in &detector.attack_patterns {
            if !pattern.enabled { continue; }
            
            let mut score = 0.0;
            for indicator in &pattern.indicators {
                if self.check_indicator(&tx, indicator).await {
                    score += 1.0;
                }
            }
            
            if score >= pattern.indicators.len() as f64 * 0.7 {
                detected.push(pattern.clone());
            }
        }
        
        if !detected.is_empty() {
            let mut det = self.mev_detector.write().await;
            det.detected_attacks += detected.len() as u64;
            det.prevented_attacks += detected.len() as u64;
        }
        
        Ok(detected)
    }

    async fn check_indicator(&self, tx: &PendingTx, indicator: &str) -> bool {
        match indicator {
            "large_buy" => tx.value > 100_000.0,
            "immediate_sell" => false,
            "same_block" => false,
            "high_gas" => tx.gas_price > 100_000_000_000,
            "known_pool" => tx.to.len() == 42,
            "pre_execution" => false,
            "known_result" => false,
            "arbitrage" => false,
            _ => false,
        }
    }

    pub async fn get_order(&self, order_id: &str) -> Option<AdvancedOrder> {
        self.orders.read().await.get(order_id).cloned()
    }

    pub async fn get_execution_reports(&self, order_id: &str) -> Vec<ExecutionReport> {
        self.execution_reports.read().await
            .iter()
            .filter(|r| r.order_id == order_id)
            .cloned()
            .collect()
    }

    pub async fn get_metrics(&self) -> ExecutionMetrics {
        self.metrics.read().await.clone()
    }
}

impl Clone for ExecutionEngine {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            orders: self.orders.clone(),
            execution_reports: self.execution_reports.clone(),
            pending_routes: self.pending_routes.clone(),
            active_algos: self.active_algos.clone(),
            metrics: self.metrics.clone(),
            mev_detector: self.mev_detector.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_execution_engine() {
        let engine = ExecutionEngine::new(ExecutionConfig::default());
        
        let order = AdvancedOrder {
            order_id: "test_1".to_string(),
            client_order_id: "client_1".to_string(),
            participant_id: "trader_1".to_string(),
            symbol: "BTC/USD".to_string(),
            side: OrderSide::Buy,
            order_type: OrderType::Market,
            quantity: 1.0,
            filled_quantity: 0.0,
            remaining_quantity: 1.0,
            price: Some(50000.0),
            stop_price: None,
            limit_price: None,
            trailing_offset: None,
            time_in_force: TimeInForce::IOC,
            status: OrderStatus::New,
            execution_instructions: ExecutionInstructions::default(),
            algo_params: AlgoParams::default(),
            legs: vec![],
            parent_order_id: None,
            child_orders: vec![],
            tags: HashMap::new(),
            created_at: 0,
            updated_at: 0,
            expires_at: None,
        };
        
        let report = engine.submit_order(order).await.unwrap();
        assert_eq!(report.status, OrderStatus::Filled);
        assert!(report.filled_quantity > 0.0);
    }
}