#![allow(dead_code)]
use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;

use crate::cloud::{BillingMeter, CloudOrchestrator};
use crate::metrics::MetricsCollector;

#[derive(Debug, Clone, Serialize)]
pub struct DashboardSnapshot {
    pub tps: u64,
    pub peak_tps: u64,
    pub total_orders: u64,
    pub total_trades: u64,
    pub active_tenants: usize,
    pub engines: usize,
    pub mrr_cents: u64,
    pub outstanding_cents: u64,
    pub uptime_secs: u64,
    pub pipeline_latency_ns: u64,
    pub pipeline_burst: bool,
    pub health: bool,
    pub timestamp: i64,
}

impl DashboardSnapshot {
    pub fn collect(
        metrics: &MetricsCollector,
        billing: &BillingMeter,
        orchestrator: &CloudOrchestrator,
    ) -> Self {
        let summary = metrics.snapshot();
        let billing_summary = billing.global_summary();
        let tps = summary["tps_current"].as_u64().unwrap_or(0);
        let peak = summary["tps_peak"].as_u64().unwrap_or(0);
        let orders = summary["trades"].as_u64().unwrap_or(0);
        let trades = summary["trades"].as_u64().unwrap_or(0);
        let uptime = summary["uptime_secs"].as_u64().unwrap_or(0);
        let health = summary["health"].as_bool().unwrap_or(false);

        DashboardSnapshot {
            tps,
            peak_tps: peak,
            total_orders: orders,
            total_trades: trades,
            active_tenants: orchestrator.tenants.tenant_count(),
            engines: orchestrator.hosts.len(),
            mrr_cents: billing_summary.monthly_recurring_cents,
            outstanding_cents: billing_summary.outstanding_cents,
            uptime_secs: uptime,
            pipeline_latency_ns: 0,
            pipeline_burst: false,
            health,
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }
}

pub async fn handle_ws_dashboard(
    ws: WebSocket,
    metrics: Arc<MetricsCollector>,
    billing: Arc<BillingMeter>,
    orchestrator: Arc<CloudOrchestrator>,
) {
    let (mut sender, _receiver) = ws.split();
    let mut interval = tokio::time::interval(Duration::from_secs(1));

    loop {
        interval.tick().await;
        let snapshot = DashboardSnapshot::collect(&metrics, &billing, &orchestrator);
        let json = serde_json::to_string(&snapshot).unwrap_or_default();
        if sender.send(Message::Text(json.into())).await.is_err() {
            break;
        }
    }
}

pub const DASHBOARD_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>THE-BRIDGE · Dashboard</title>
<style>
  * { margin: 0; padding: 0; box-sizing: border-box; }
  body { background: #0b0e17; color: #c9d1d9; font-family: 'SF Mono','Consolas','Courier New',monospace; font-size: 13px; padding: 24px; }
  h1 { color: #00d4aa; font-size: 18px; margin-bottom: 20px; letter-spacing: 0.5px; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(200px, 1fr)); gap: 12px; }
  .card { background: #131a2b; border: 1px solid #1e2a45; border-radius: 8px; padding: 16px; }
  .card .label { color: #6b7a8f; font-size: 10px; text-transform: uppercase; letter-spacing: 0.8px; margin-bottom: 6px; }
  .card .value { color: #e6edf3; font-size: 22px; font-weight: 600; }
  .card .value.green { color: #00d4aa; }
  .card .value.red { color: #f85149; }
  .card .value.yellow { color: #d29922; }
  .status-bar { margin-top: 20px; padding: 10px 16px; background: #131a2b; border: 1px solid #1e2a45; border-radius: 8px; display: flex; gap: 24px; }
  .status-bar span { font-size: 11px; color: #6b7a8f; }
  .status-bar strong { color: #e6edf3; }
  .badge { display: inline-block; padding: 2px 8px; border-radius: 4px; font-size: 10px; }
  .badge.online { background: #00d4aa22; color: #00d4aa; border: 1px solid #00d4aa44; }
  .badge.burst { background: #d2992222; color: #d29922; border: 1px solid #d2992244; }
</style>
</head>
<body>
<h1>⬡ THE-BRIDGE · DASHBOARD</h1>
<div class="grid" id="metrics"></div>
<div class="status-bar" id="status">connecting...</div>
<script>
  const ws = new WebSocket((location.protocol==='https:'?'wss:':'ws:')+'//'+location.host+'/ws/dashboard');
  ws.onmessage = function(e) {
    const d = JSON.parse(e.data);
    document.getElementById('metrics').innerHTML = `
      <div class="card"><div class="label">TPS Current</div><div class="value green">${d.tps.toLocaleString()}</div></div>
      <div class="card"><div class="label">TPS Peak</div><div class="value yellow">${d.peak_tps.toLocaleString()}</div></div>
      <div class="card"><div class="label">Total Orders</div><div class="value">${d.total_orders.toLocaleString()}</div></div>
      <div class="card"><div class="label">Total Trades</div><div class="value">${d.total_trades.toLocaleString()}</div></div>
      <div class="card"><div class="label">Active Tenants</div><div class="value">${d.active_tenants}</div></div>
      <div class="card"><div class="label">Engines</div><div class="value">${d.engines}</div></div>
      <div class="card"><div class="label">MRR</div><div class="value green">$${(d.mrr_cents/100).toFixed(2)}</div></div>
      <div class="card"><div class="label">Outstanding</div><div class="value">$${(d.outstanding_cents/100).toFixed(2)}</div></div>
    `;
    document.getElementById('status').innerHTML = `
      <span>uptime: <strong>${Math.floor(d.uptime_secs/3600)}h ${Math.floor((d.uptime_secs%3600)/60)}m</strong></span>
      <span>health: <strong class="${d.health?'green':'red'}">${d.health?'operational':'halted'}</strong></span>
      <span>pipeline: <span class="badge ${d.pipeline_burst?'burst':'online'}">${d.pipeline_burst?'BURST':'normal'}</span></span>
      <span>last: <strong>${new Date(d.timestamp).toLocaleTimeString()}</strong></span>
    `;
  };
  ws.onclose = function() { document.getElementById('status').innerHTML = 'disconnected — <a href="/dashboard" style="color:#00d4aa">reload</a>'; };
</script>
</body>
</html>"##;
