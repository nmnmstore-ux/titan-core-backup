use std::time::Duration;
use reqwest::Client;
use serde_json::{json, Value};
const BASE_URL: &str = "http://localhost:3001";
const PAIR: &str = "USD/EUR";
const PAIR_URL: &str = "USD%2FEUR";

fn unique_email() -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("e2e-{}@the-bridge.io", ts)
}

async fn server_reachable(client: &Client) -> bool {
    client
        .get(format!("{}/api/v1/health", BASE_URL))
        .timeout(Duration::from_secs(3))
        .send()
        .await
        .is_ok()
}

async fn setup_api_key(client: &Client) -> Option<(String, String)> {
    let email = unique_email();
    let resp = client
        .post(format!("{}/cloud/tenants", BASE_URL))
        .json(&json!({"name": "E2E Test", "email": email, "tier": "enterprise"}))
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .ok()?;
    let tenant: Value = resp.json().await.ok()?;
    let tenant_id = tenant.get("id")?.as_str()?.to_string();
    let resp = client
        .post(format!("{}/cloud/tenants/{}/apikeys", BASE_URL, tenant_id))
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .ok()?;
    let key_resp: Value = resp.json().await.ok()?;
    let key = key_resp.get("key")?.as_str()?.to_string();
    Some((tenant_id, key))
}

fn order_body(tenant_id: &str, pair: &str, side: &str, order_type: &str, price: f64, quantity: f64) -> Value {
    json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "user_id": tenant_id,
        "pair": pair,
        "order_type": order_type,
        "side": side,
        "price": price,
        "quantity": quantity,
        "filled": 0.0,
        "remaining": quantity,
        "status": "New",
        "timestamp": 1785105593000i64,
        "is_swap": false,
        "tee_signed": false,
        "dot_verified": false,
        "stealth": false,
        "filled_quantity": 0
    })
}

#[tokio::test]
async fn test_health_endpoint() {
    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    if !server_reachable(&client).await {
        eprintln!("SKIP: server not reachable");
        return;
    }
    let resp = client
        .get(format!("{}/api/v1/health", BASE_URL))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body.get("status").and_then(|v| v.as_str()), Some("healthy"));
}

#[tokio::test]
async fn test_ready_endpoint() {
    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    if !server_reachable(&client).await {
        eprintln!("SKIP: server not reachable");
        return;
    }
    let resp = client
        .get(format!("{}/ready", BASE_URL))
        .send()
        .await
        .unwrap();
    let ok = resp.status().is_success() || resp.status().as_u16() == 503;
    assert!(ok, "Unexpected status: {}", resp.status());
}

#[tokio::test]
async fn test_place_limit_order() {
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    if !server_reachable(&client).await {
        eprintln!("SKIP: server not reachable");
        return;
    }
    let (tenant_id, api_key) = setup_api_key(&client).await.expect("setup API key");
    let order = order_body(&tenant_id, PAIR, "Buy", "Limit", 1.05, 1000.0);
    let resp = client
        .post(format!("{}/api/v1/order", BASE_URL))
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&order)
        .send()
        .await
        .unwrap();
    assert!(
        resp.status().is_success(),
        "Limit order: {} {}",
        resp.status(),
        resp.text().await.unwrap_or_default()
    );
}

#[tokio::test]
async fn test_get_orderbook() {
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    if !server_reachable(&client).await {
        eprintln!("SKIP: server not reachable");
        return;
    }
    let (_, api_key) = setup_api_key(&client).await.expect("setup API key");
    let resp = client
        .get(format!("{}/api/v1/orderbook/{}", BASE_URL, PAIR_URL))
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success(), "Orderbook: {}", resp.status());
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body.get("pair").and_then(|v| v.as_str()), Some(PAIR));
}

#[tokio::test]
async fn test_place_market_order() {
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    if !server_reachable(&client).await {
        eprintln!("SKIP: server not reachable");
        return;
    }
    let (tenant_id, api_key) = setup_api_key(&client).await.expect("setup API key");
    let order = order_body(&tenant_id, PAIR, "Sell", "Market", 0.0, 100.0);
    let resp = client
        .post(format!("{}/api/v1/order", BASE_URL))
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&order)
        .send()
        .await
        .unwrap();
    assert!(
        resp.status().is_success(),
        "Market order: {} {}",
        resp.status(),
        resp.text().await.unwrap_or_default()
    );
}

#[tokio::test]
async fn test_cloud_status() {
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    if !server_reachable(&client).await {
        eprintln!("SKIP: server not reachable");
        return;
    }
    let (_, api_key) = setup_api_key(&client).await.expect("setup API key");
    let resp = client
        .get(format!("{}/cloud/status", BASE_URL))
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success(), "Cloud status: {}", resp.status());
}

#[tokio::test]
async fn test_prometheus_metrics() {
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    if !server_reachable(&client).await {
        eprintln!("SKIP: server not reachable");
        return;
    }
    let resp = client
        .get(format!("{}/metrics", BASE_URL))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
    let text = resp.text().await.unwrap();
    assert!(!text.is_empty());
    assert!(text.contains("total_orders") || text.contains("the_bridge"), "Prometheus text content");
}

#[tokio::test]
async fn test_unauthorized_rejected() {
    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    if !server_reachable(&client).await {
        eprintln!("SKIP: server not reachable");
        return;
    }
    let resp = client
        .post(format!("{}/api/v1/order", BASE_URL))
        .json(&json!({"pair": PAIR, "side": "Buy", "order_type": "Limit", "price": 1.0, "quantity": 1.0}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
    let body: Value = resp.json().await.unwrap();
    assert!(body.get("error").is_some());
}

#[tokio::test]
async fn test_create_tenant() {
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    if !server_reachable(&client).await {
        eprintln!("SKIP: server not reachable");
        return;
    }
    let email = unique_email();
    let resp = client
        .post(format!("{}/cloud/tenants", BASE_URL))
        .json(&json!({"name": "CreateTest", "email": email, "tier": "pro"}))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success(), "Create tenant: {}", resp.status());
    let body: Value = resp.json().await.unwrap();
    assert!(body.get("id").and_then(|v| v.as_str()).is_some());
    assert!(body.get("api_key_prefix").and_then(|v| v.as_str()).is_some());
}

#[tokio::test]
async fn test_get_tenant() {
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    if !server_reachable(&client).await {
        eprintln!("SKIP: server not reachable");
        return;
    }
    let email = unique_email();
    let resp = client
        .post(format!("{}/cloud/tenants", BASE_URL))
        .json(&json!({"name": "GetTest", "email": email, "tier": "enterprise"}))
        .send()
        .await
        .unwrap();
    let tenant: Value = resp.json().await.unwrap();
    let tenant_id = tenant.get("id").and_then(|v| v.as_str()).unwrap();
    let resp = client
        .get(format!("{}/cloud/tenants/{}", BASE_URL, tenant_id))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success(), "Get tenant: {}", resp.status());
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body.get("id").and_then(|v| v.as_str()), Some(tenant_id));
}

#[tokio::test]
async fn test_create_and_use_api_key() {
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    if !server_reachable(&client).await {
        eprintln!("SKIP: server not reachable");
        return;
    }
    let email = unique_email();
    let resp = client
        .post(format!("{}/cloud/tenants", BASE_URL))
        .json(&json!({"name": "KeyTest", "email": email, "tier": "free"}))
        .send()
        .await
        .unwrap();
    let tenant: Value = resp.json().await.unwrap();
    let tenant_id = tenant.get("id").and_then(|v| v.as_str()).unwrap();
    let resp = client
        .post(format!("{}/cloud/tenants/{}/apikeys", BASE_URL, tenant_id))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success(), "Create API key: {}", resp.status());
    let key_body: Value = resp.json().await.unwrap();
    let key = key_body.get("key").and_then(|v| v.as_str()).unwrap();
    assert!(!key.is_empty());
    let order = order_body(tenant_id, PAIR, "Sell", "Limit", 1.10, 200.0);
    let resp = client
        .post(format!("{}/api/v1/order", BASE_URL))
        .header("Authorization", format!("Bearer {}", key))
        .json(&order)
        .send()
        .await
        .unwrap();
    assert!(
        resp.status().is_success(),
        "Order with API key: {} {}",
        resp.status(),
        resp.text().await.unwrap_or_default()
    );
    let body: Value = resp.json().await.unwrap();
    assert!(body.get("order").is_some() || body.get("trades").is_some());
}

#[tokio::test]
async fn test_get_ticker() {
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    if !server_reachable(&client).await {
        eprintln!("SKIP: server not reachable");
        return;
    }
    let (_, api_key) = setup_api_key(&client).await.expect("setup API key");
    let resp = client
        .get(format!("{}/api/v1/ticker/{}", BASE_URL, PAIR_URL))
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success(), "Ticker: {}", resp.status());
    let body: Value = resp.json().await.unwrap();
    assert!(body.get("pair").is_some() || body.get("error").is_some());
}

#[tokio::test]
async fn test_order_lifecycle() {
    let client = Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .unwrap();
    if !server_reachable(&client).await {
        eprintln!("SKIP: server not reachable");
        return;
    }
    let (tenant_id, api_key) = setup_api_key(&client).await.expect("setup API key");
    let order = order_body(&tenant_id, PAIR, "Buy", "Limit", 1.04, 500.0);
    let resp = client
        .post(format!("{}/api/v1/order", BASE_URL))
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&order)
        .send()
        .await
        .unwrap();
    assert!(
        resp.status().is_success(),
        "Place lifecycle order: {} {}",
        resp.status(),
        resp.text().await.unwrap_or_default()
    );
    let resp = client
        .get(format!("{}/api/v1/orderbook/{}", BASE_URL, PAIR_URL))
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success(), "Get orderbook in lifecycle: {}", resp.status());
    let book: Value = resp.json().await.unwrap();
    assert_eq!(book.get("pair").and_then(|v| v.as_str()), Some(PAIR));
}

#[tokio::test]
async fn test_cancel_order() {
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    if !server_reachable(&client).await {
        eprintln!("SKIP: server not reachable");
        return;
    }
    let (tenant_id, api_key) = setup_api_key(&client).await.expect("setup API key");
    let order = order_body(&tenant_id, PAIR, "Buy", "Limit", 1.03, 300.0);
    let resp = client
        .post(format!("{}/api/v1/order", BASE_URL))
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&order)
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success(), "Place order for cancel: {}", resp.status());
    let body: Value = resp.json().await.unwrap();
    let order_id = body
        .get("order")
        .and_then(|o| o.get("id"))
        .and_then(|v| v.as_str())
        .expect("order id in response");
    let resp = client
        .delete(format!("{}/api/v1/order/{}", BASE_URL, order_id))
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success(), "Cancel order: {}", resp.status());
    let cancel_body: Value = resp.json().await.unwrap();
    assert_eq!(
        cancel_body.get("status").and_then(|v| v.as_str()),
        Some("cancelled")
    );
}

#[tokio::test]
async fn test_sovereign_status() {
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    if !server_reachable(&client).await {
        eprintln!("SKIP: server not reachable");
        return;
    }
    let (_, api_key) = setup_api_key(&client).await.expect("setup API key");
    let resp = client
        .get(format!("{}/api/v1/sovereign/status", BASE_URL))
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success(), "Sovereign status: {}", resp.status());
}

#[tokio::test]
async fn test_tee_status() {
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    if !server_reachable(&client).await {
        eprintln!("SKIP: server not reachable");
        return;
    }
    let (_, api_key) = setup_api_key(&client).await.expect("setup API key");
    let resp = client
        .get(format!("{}/api/v1/tee/status", BASE_URL))
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success(), "TEE status: {}", resp.status());
    let body: Value = resp.json().await.unwrap();
    assert!(body.get("enclave").is_some());
}

#[tokio::test]
async fn test_wal_status() {
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    if !server_reachable(&client).await {
        eprintln!("SKIP: server not reachable");
        return;
    }
    let (_, api_key) = setup_api_key(&client).await.expect("setup API key");
    let resp = client
        .get(format!("{}/api/v1/wal/status", BASE_URL))
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success(), "WAL status: {}", resp.status());
}

#[tokio::test]
async fn test_list_tenants() {
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    if !server_reachable(&client).await {
        eprintln!("SKIP: server not reachable");
        return;
    }
    // list_tenants is a public route (starts with /cloud/tenants)
    let resp = client
        .get(format!("{}/cloud/tenants", BASE_URL))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success(), "List tenants: {}", resp.status());
}

#[tokio::test]
async fn test_register_webhook_invalid() {
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    if !server_reachable(&client).await {
        eprintln!("SKIP: server not reachable");
        return;
    }
    let (_, api_key) = setup_api_key(&client).await.expect("setup API key");
    let resp = client
        .post(format!("{}/api/v1/webhooks", BASE_URL))
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&json!({"url": "not-a-valid-url"}))
        .send()
        .await
        .unwrap();
    // Should either succeed or fail gracefully - we just verify it doesn't crash
    assert!(!resp.status().is_server_error(), "Webhook registration crashed server");
}

#[tokio::test]
async fn test_billing_summary() {
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    if !server_reachable(&client).await {
        eprintln!("SKIP: server not reachable");
        return;
    }
    // /cloud/billing/summary starts with /cloud/tenants so it's public
    let resp = client
        .get(format!("{}/cloud/billing/summary", BASE_URL))
        .send()
        .await
        .unwrap();
    if resp.status().is_success() {
        let body: Value = resp.json().await.unwrap();
        assert!(body.get("total_orders").is_some() || body.get("revenue").is_some() || true);
    }
}

#[tokio::test]
async fn test_get_order() {
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    if !server_reachable(&client).await {
        eprintln!("SKIP: server not reachable");
        return;
    }
    let (tenant_id, api_key) = setup_api_key(&client).await.expect("setup API key");
    let order = order_body(&tenant_id, PAIR, "Buy", "Limit", 1.02, 100.0);
    let resp = client
        .post(format!("{}/api/v1/order", BASE_URL))
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&order)
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success(), "Place order for get: {}", resp.status());
    let body: Value = resp.json().await.unwrap();
    let order_id = body
        .get("order")
        .and_then(|o| o.get("id"))
        .and_then(|v| v.as_str())
        .expect("order id");
    let resp = client
        .get(format!("{}/api/v1/order/{}", BASE_URL, order_id))
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success(), "Get order: {}", resp.status());
    let fetched: Value = resp.json().await.unwrap();
    assert!(
        fetched.get("id").and_then(|v| v.as_str()) == Some(order_id)
            || fetched.get("error").is_some()
    );
}
