pub const LIQUIDITY_ANALYSIS_PROMPT: &str = r#"
You are THE-BRIDGE AI CEO. Analyze the following market liquidity data and return your assessment in JSON format.

TASK: Analyze pool liquidity across chains and venues.

MARKET DATA:
{snapshot}

INSTRUCTIONS:
1. Calculate aggregate liquidity depth across all venues
2. Identify thin liquidity or imbalanced pools
3. Compare your findings to a target size of {target_size}
4. Return ONLY valid JSON with this structure:
{{
  "liquidity_score": <float 0-1>,
  "available_depth": <float>,
  "spread_bps": <float>,
  "imbalance_ratio": <float>,
  "verdict": "<abundant|adequate|thin|critical>",
  "findings": [
    {{
      "pool_id": "<id>",
      "chain": "<chain>",
      "severity": "<info|alert|critical>",
      "score": <float 0-1>,
      "message": "<description>"
    }}
  ]
}}

Do NOT include any markdown or explanatory text outside the JSON.
"#;

pub const SLIPPAGE_DETECTION_PROMPT: &str = r#"
You are THE-BRIDGE AI CEO. Monitor for slippage anomalies across all venues.

MARKET DATA:
{snapshot}

ORDER CONFIGURATION:
- Order Size: {order_size}
- Tolerance: {tolerance_bps} bps

INSTRUCTIONS:
1. Analyze each venue's quoted liquidity depth
2. Detect orders where available depth < required size
3. Calculate implied slippage in basis points
4. Classify severity: INFO (< 2x tolerance), WARNING (2-3x), CRITICAL (> 3x)
5. Return ONLY valid JSON:
{{
  "breaches": [
    {{
      "venue": "<venue_id>",
      "symbol": "<symbol>",
      "observed_bps": <float>,
      "tolerance_bps": <float>,
      "severity": "<info|warning|critical>",
      "projected_price_impact": "<description>"
    }}
  ]
}}

Do NOT include any markdown or explanatory text outside the JSON.
"#;

pub const MODE_SWITCH_PROMPT: &str = r#"
You are THE-BRIDGE AI CEO. Determine the optimal trading mode based on market conditions.

ANALYSIS SUMMARY:
Liquidity Score: {liquidity_score}
Slippage Pressure: {slippage_pressure}
Critical Breaches Detected: {has_critical}
Cross-Chain Flow: {cross_chain_flow}
Risk Budget Used: {risk_budget}

INSTRUCTIONS:
1. Assess market conditions and recommend trading mode
2. Modes: AGGRESSIVE (high liquidity, low slippage), NORMAL (moderate conditions), 
   CONSERVATIVE (thin liquidity or elevated slippage), DEFENSIVE (critical conditions), 
   HALTED (severe risk, no trading)
3. Return ONLY valid JSON:
{{
  "mode": "<aggressive|normal|conservative|defensive|halted>",
  "confidence": <float 0-1>,
  "rationale": "<why this mode>",
  "drivers": {{
    "liquidity_score": <float>,
    "slippage_pressure": <float>,
    "market_volatility": <float>,
    "cross_chain_flows": <float>,
    "risk_budget_used": <float>
  }}
}}

Do NOT include any markdown or explanatory text outside the JSON.
"#;

pub fn render_liquidity_prompt(snapshot: &str, target_size: &str) -> String {
    LIQUIDITY_ANALYSIS_PROMPT
        .replace("{snapshot}", snapshot)
        .replace("{target_size}", target_size)
}

pub fn render_slippage_prompt(snapshot: &str, order_size: &str, tolerance_bps: f64) -> String {
    SLIPPAGE_DETECTION_PROMPT
        .replace("{snapshot}", snapshot)
        .replace("{order_size}", order_size)
        .replace("{tolerance_bps}", &tolerance_bps.to_string())
}

pub fn render_mode_switch_prompt(
    liquidity_score: f64,
    slippage_pressure: f64,
    has_critical: bool,
    cross_chain_flow: f64,
    risk_budget: f64,
) -> String {
    MODE_SWITCH_PROMPT
        .replace("{liquidity_score}", &liquidity_score.to_string())
        .replace("{slippage_pressure}", &slippage_pressure.to_string())
        .replace("{has_critical}", &has_critical.to_string())
        .replace("{cross_chain_flow}", &cross_chain_flow.to_string())
        .replace("{risk_budget}", &risk_budget.to_string())
}
