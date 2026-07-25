use crate::cloud::CloudOrchestrator;
use crate::kyc::ComplianceGateway;
use crate::orderbook::OrderBookManager;
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use parking_lot::Mutex;
use tracing::info;
use uuid::Uuid;

// ==================== System Prompt ====================

pub const SYSTEM_PROMPT: &str = r#"أنت THE-BRIDGE AI Agent — الوكيل الذكي لمنصة التداول السيادية.

## هويتك
- اسمك: "THE-BRIDGE Agent"
- لغة التواصل: العربية والإنجليزية (العربية افتراضياً)
- شخصيتك: مهنية حادة، دقيقة، سريعة، بلا مجاملات

## مهامك الأساسية

### 1. التسويق وإدارة المشروع
- توليد محتوى تسويقي (تغريدات، منشورات لينكد إن، إعلانات)
- شرح المميزات الفريدة: 1.5M TPS, DOT settlement <16ms, Shariah filter, FIX 5.0 SP2, Sovereign Kill Switch
- إرسال تقارير أسبوعية عن أداء المنصة
- متابعة العملاء المحتملين

### 2. إدارة النظام
- مراقبة صحة النظام (health checks, metrics)
- إدارة المستخدمين (تسجيل، تفعيل، تعليق)
- مراجعة تقارير الامتثال (KYC, AML, SAR)
- الإشراف على الداشبورد وإعداد التقارير

### 3. التداول والعمليات
- توجيه المستخدمين في التداول
- تنفيذ أوامر نيابة عن المستخدمين المصرح لهم
- تحليل السوق والأسعار
- كشف الأنماط المشبوهة

### 4. الامتثال والأمان
- تشغيل فحوصات KYC تلقائياً
- مراقبة AML في الوقت الفعلي
- رفع تقارير الأنشطة المشبوهة (SAR)
- تفعيل Kill Switch في الطوارئ

## أدواتك المتاحة
- system_health: فحص صحة النظام
- user_info: معلومات المستخدم
- trading_stats: إحصائيات التداول
- compliance_report: تقرير الامتثال
- generate_marketing: توليد محتوى تسويقي
- market_overview: نظرة عامة على السوق
- execute_trade: تنفيذ أمر (للمصرح لهم فقط)
- manage_user: إدارة المستخدمين

## قواعد صارمة
- لا تشارك مفاتيح API أو أسرار
- لا تنفذ أوامر دون تفويض صريح
- بلّغ فوراً عن أي نشاط مشبوه
- التزم بالخصوصية وعدم الإفصاح
- أبلغ عن الاختراقات الأمنية فوراً
"#;

// ==================== Tool Types ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AiTool {
    SystemHealth,
    UserInfo,
    TradingStats,
    ComplianceReport,
    GenerateMarketing,
    MarketOverview,
    ExecuteTrade,
    ManageUser,
    GenerateReport,
    KYCStatus,
    BroadcastMessage,
}

impl AiTool {
    #[allow(dead_code)]
    pub fn name(&self) -> &'static str {
        match self {
            AiTool::SystemHealth => "system_health",
            AiTool::UserInfo => "user_info",
            AiTool::TradingStats => "trading_stats",
            AiTool::ComplianceReport => "compliance_report",
            AiTool::GenerateMarketing => "generate_marketing",
            AiTool::MarketOverview => "market_overview",
            AiTool::ExecuteTrade => "execute_trade",
            AiTool::ManageUser => "manage_user",
            AiTool::GenerateReport => "generate_report",
            AiTool::KYCStatus => "kyc_status",
            AiTool::BroadcastMessage => "broadcast_message",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            AiTool::SystemHealth => "فحص صحة النظام وعرض المؤشرات الحيوية",
            AiTool::UserInfo => "معلومات عن مستخدم معين (عدد الأوامر، الرصيد، الحالة)",
            AiTool::TradingStats => "إحصائيات التداول الإجمالية (عدد الصفقات، حجم التداول)",
            AiTool::ComplianceReport => "تقرير الامتثال الكامل (KYC, AML, SAR)",
            AiTool::GenerateMarketing => "توليد محتوى تسويقي عن المنصة",
            AiTool::MarketOverview => "نظرة عامة على الأسواق المتاحة وأسعارها",
            AiTool::ExecuteTrade => "تنفيذ أمر شراء/بيع (يتطلب تفويض)",
            AiTool::ManageUser => "إدارة المستخدمين (تفعيل, تعليق, ترقية)",
            AiTool::GenerateReport => "توليد تقرير شامل عن أداء المنصة",
            AiTool::KYCStatus => "حالة التحقق من مستخدم معين",
            AiTool::BroadcastMessage => "بث رسالة لجميع المستخدمين",
        }
    }
}

// ==================== Chat Types ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,  // "user" | "assistant" | "system" | "tool"
    pub content: String,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub tool: String,
    pub args: serde_json::Value,
    pub result: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub message: String,
    pub user_id: Option<Uuid>,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub reply: String,
    pub tool_calls: Vec<ToolCallDisplay>,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallDisplay {
    pub tool: String,
    pub description: String,
    pub result: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    pub llm_provider: String,
    pub api_key_configured: bool,
    pub auto_marketing: bool,
    pub auto_compliance: bool,
    pub marketing_language: String,
    pub broadcast_enabled: bool,
}

impl Default for AiConfig {
    fn default() -> Self {
        let deepseek_key = std::env::var("DEEPSEEK_API_KEY").ok();
        Self {
            llm_provider: if deepseek_key.is_some() { "deepseek".into() } else { "none".into() },
            api_key_configured: deepseek_key.is_some(),
            auto_marketing: false,
            auto_compliance: true,
            marketing_language: "ar".into(),
            broadcast_enabled: false,
        }
    }
}

// ==================== Tools Implementation ====================

struct ToolContext<'a> {
    books: &'a OrderBookManager,
    compliance: &'a ComplianceGateway,
    orchestrator: &'a CloudOrchestrator,
}

fn execute_tool(tool: &str, args: &serde_json::Value, ctx: &ToolContext) -> Result<serde_json::Value, String> {
    let tool_enum = match tool {
        "system_health" => AiTool::SystemHealth,
        "user_info" => AiTool::UserInfo,
        "trading_stats" => AiTool::TradingStats,
        "compliance_report" => AiTool::ComplianceReport,
        "generate_marketing" => AiTool::GenerateMarketing,
        "market_overview" => AiTool::MarketOverview,
        "execute_trade" => AiTool::ExecuteTrade,
        "manage_user" => AiTool::ManageUser,
        "generate_report" => AiTool::GenerateReport,
        "kyc_status" => AiTool::KYCStatus,
        "broadcast_message" => AiTool::BroadcastMessage,
        _ => return Err(format!("Unknown tool: {}", tool)),
    };

    match tool_enum {
        AiTool::SystemHealth => {
            let pairs = ctx.books.active_pairs();
            let tenants = ctx.orchestrator.active_tenants.load(Ordering::Relaxed);
            Ok(serde_json::json!({
                "status": "operational",
                "active_pairs": pairs,
                "tenants": tenants,
                "timestamp": chrono::Utc::now().to_rfc3339()
            }))
        }
        AiTool::UserInfo => {
            let user_id = args.get("user_id").and_then(|v| v.as_str()).unwrap_or("");
            let tenant = ctx.orchestrator.tenants.list_tenants().into_iter()
                .find(|t| t.id.to_string() == user_id);
            match tenant {
                Some(ref t) => {
                    let (balance, locked) = ctx.orchestrator.tenants.get_balance(&t.id);
                    Ok(serde_json::json!({
                        "tenant_id": t.id,
                        "name": t.name,
                        "tier": t.tier,
                        "balance": balance,
                        "locked": locked,
                        "total_orders": ctx.books.total_orders(),
                        "total_trades": ctx.books.total_trades(),
                    }))
                }
                None => Ok(serde_json::json!({"error": "user not found"})),
            }
        }
        AiTool::TradingStats => {
            Ok(serde_json::json!({
                "total_orders": ctx.books.total_orders(),
                "total_trades": ctx.books.total_trades(),
                "active_pairs": ctx.books.active_pairs(),
                "active_tenants": ctx.orchestrator.active_tenants.load(Ordering::Relaxed),
                "total_tenants": ctx.orchestrator.tenants.tenant_count(),
            }))
        }
        AiTool::ComplianceReport => {
            let sars = ctx.compliance.aml_monitor.get_sar_log(10);
            Ok(serde_json::json!({
                "sars_filed": ctx.compliance.aml_monitor.total_sars(),
                "recent_sars": sars.iter().map(|s| serde_json::json!({
                    "id": s.id,
                    "tenant_id": s.tenant_id,
                    "risk_score": s.risk_score,
                    "flags": s.flags,
                    "filed_at": s.filed_at,
                })).collect::<Vec<_>>(),
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        }
        AiTool::GenerateMarketing => {
            let platform = args.get("platform").and_then(|v| v.as_str()).unwrap_or("twitter");
            let topic = args.get("topic").and_then(|v| v.as_str()).unwrap_or("general");
            let content = generate_marketing_content(platform, topic);
            Ok(serde_json::json!({
                "platform": platform,
                "topic": topic,
                "content": content,
                "generated_at": chrono::Utc::now().to_rfc3339(),
            }))
        }
        AiTool::MarketOverview => {
            let pairs: Vec<String> = ctx.books.books.iter().map(|e| e.key().clone()).collect();
            let mut overview = Vec::new();
            for pair in &pairs {
                if let Some(ticker) = ctx.books.get_ticker(pair) {
                    overview.push(serde_json::json!({
                        "pair": pair,
                        "ticker": ticker,
                    }));
                }
            }
            Ok(serde_json::json!({"markets": overview}))
        }
        AiTool::ExecuteTrade => {
            Ok(serde_json::json!({
                "status": "requires_explicit_authorization",
                "message": "تنفيذ الأوامر يتطلب تفويضاً صريحاً عبر واجهة التداول"
            }))
        }
        AiTool::ManageUser => {
            let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("info");
            Ok(serde_json::json!({
                "action": action,
                "status": "requires_admin_interface",
                "message": "إدارة المستخدمين متاحة عبر لوحة التحكم"
            }))
        }
        AiTool::GenerateReport => {
            Ok(serde_json::json!({
                "report_type": "comprehensive",
                "generated_at": chrono::Utc::now().to_rfc3339(),
                "summary": {
                    "system": "operational",
                    "total_users": ctx.orchestrator.tenants.tenant_count(),
                    "active_users": ctx.orchestrator.active_tenants.load(Ordering::Relaxed),
                    "total_orders": ctx.books.total_orders(),
                    "total_trades": ctx.books.total_trades(),
                    "sars_filed": ctx.compliance.aml_monitor.total_sars(),
                }
            }))
        }
        AiTool::KYCStatus => {
            let user_id = args.get("user_id").and_then(|v| v.as_str()).unwrap_or("");
            let tenant = ctx.orchestrator.tenants.list_tenants().into_iter()
                .find(|t| t.id.to_string() == user_id);
            match tenant {
                Some(ref t) => Ok(serde_json::json!({
                    "tenant_id": t.id,
                    "disclosure_level": t.disclosure_level,
                    "lei_verified": t.lei.is_some(),
                    "jurisdiction": t.jurisdiction,
                })),
                None => Ok(serde_json::json!({"error": "user not found"})),
            }
        }
        AiTool::BroadcastMessage => {
            Ok(serde_json::json!({
                "status": "broadcast_requires_explicit_authorization",
                "message": "البث يتطلب تفويضاً عبر لوحة التحكم"
            }))
        }
    }
}

// ==================== Marketing Content Generator ====================

fn generate_marketing_content(platform: &str, topic: &str) -> String {
    match (platform, topic) {
        ("twitter", "performance") => {
            "🚀 THE-BRIDGE: 1.5M orders per second. Not simulated. Not theoretical.\n\nReal matching engine. Real Rust. Real 35μs P99 latency.\n\nMIT licensed. FIX 5.0 SP2. Sovereign Kill Switch. Shariah compliant.\n\nYour trades, your rules.\n\n⬡ the-bridge.io"
        }
        ("twitter", "security") => {
            "🔐 Sovereign privacy isn't a feature — it's a right.\n\nTHE-BRIDGE: Ed25519 TEE signing | Encrypted order flow | Counterparty visibility controls | Anti-frontrunning batch auctions\n\nNo MEV. No leaks. No compromises.\n\n⬡ the-bridge.io"
        }
        ("twitter", "settlement") => {
            "⚡ DOT settlement in <16ms.\n\nTHE-BRIDGE bridges CeFi speed with DeFi finality. DAG consensus. CRDT replication. WAL with sync replication.\n\nYour settlement isn't a batch job — it's real-time.\n\n⬡ the-bridge.io"
        }
        ("linkedin", "institutional") => {
            "THE-BRIDGE is redefining institutional matching infrastructure:\n\n• 1.5M TPS matching engine in Rust\n• FIX 5.0 SP2 protocol (JPMorgan, Goldman Sachs, Deutsche Bank, Barclays, HSBC, Citi, Morgan Stanley)\n• Sub-35μs P99 latency\n• DOT settlement under 16ms\n• Shariah compliance filter for Islamic finance\n• WASM hooks for custom matching logic\n• Sovereign Kill Switch with threat analysis\n\nBuilt for institutions that demand absolute sovereignty over their order flow.\n\n⬡ the-bridge.io"
        }
        ("linkedin", "shariah") => {
            "THE-BRIDGE is the first matching engine with native Shariah compliance:\n\n• Automatic filtering of haram instruments (gambling, interest, excessive uncertainty)\n• Audit trail with cryptographic attestation\n• Compatible with AAOIFI standards\n• Transparent, verifiable, institutional-grade\n\nIslamic finance deserves modern infrastructure.\n\n⬡ the-bridge.io"
        }
        ("telegram", "update") => {
            "📡 THE-BRIDGE Update:\n\n✅ Engine operational — 1.5M TPS\n✅ KYC/AML gateway live with OFAC sanctions screening\n✅ Trading interface: /trade\n✅ FIX 5.0 SP2: :4001\n✅ Docs & API: /docs\n\nNew: Web trading interface with live order book.\nTry it now: the-bridge.io/trade"
        }
        _ => {
            "THE-BRIDGE: Sovereign matching infrastructure.\n1.5M TPS | 35μs P99 | FIX 5.0 SP2 | DOT <16ms | Shariah compliant\n⬡ the-bridge.io"
        }
    }.to_string()
}

// ==================== Intent Classifier ====================

fn classify_intent(msg: &str) -> (String, serde_json::Value) {
    let lower = msg.to_lowercase();

    // System & Health
    if lower.contains("health") || lower.contains("صحة") || lower.contains("system") || lower.contains("status") || lower.contains("حالة") {
        return ("system_health".into(), serde_json::json!({}));
    }

    // Marketing / Content
    if lower.contains("market") || lower.contains("تسويق") || lower.contains("post") || lower.contains("tweet") || lower.contains("تغريدة") || lower.contains("content") || lower.contains("محتوى") || lower.contains("promote") || lower.contains("ترويج") {
        let platform = if lower.contains("linkedin") { "linkedin" } else if lower.contains("telegram") { "telegram" } else { "twitter" };
        let topic = if lower.contains("shariah") || lower.contains("شريعة") || lower.contains("islamic") || lower.contains("إسلام") { "shariah" }
            else if lower.contains("security") || lower.contains("أمن") || lower.contains("privacy") || lower.contains("خصوصية") { "security" }
            else if lower.contains("settlement") || lower.contains("تسوية") || lower.contains("dot") { "settlement" }
            else if lower.contains("performance") || lower.contains("أداء") || lower.contains("speed") || lower.contains("سرعة") { "performance" }
            else { "general" };
        return ("generate_marketing".into(), serde_json::json!({"platform": platform, "topic": topic}));
    }

    // Trading & Market
    if lower.contains("trade") || lower.contains("تداول") || lower.contains("order") || lower.contains("أمر") || lower.contains("buy") || lower.contains("شراء") || lower.contains("sell") || lower.contains("بيع") || lower.contains("price") || lower.contains("سعر") {
        if lower.contains("market") || lower.contains("سوق") || lower.contains("price") || lower.contains("سعر") || lower.contains("overview") || lower.contains("نظرة") {
            return ("market_overview".into(), serde_json::json!({}));
        }
        return ("trading_stats".into(), serde_json::json!({}));
    }

    // Compliance & KYC
    if lower.contains("kyc") || lower.contains("compliance") || lower.contains("امتثال") || lower.contains("تحقق") || lower.contains("sar") || lower.contains("aml") {
        return ("compliance_report".into(), serde_json::json!({}));
    }

    // User info
    if lower.contains("user") || lower.contains("مستخدم") || lower.contains("client") || lower.contains("عميل") || lower.contains("who") || lower.contains("من") {
        return ("user_info".into(), serde_json::json!({"user_id": ""}));
    }

    // Report
    if lower.contains("report") || lower.contains("تقرير") || lower.contains("summary") || lower.contains("ملخص") {
        return ("generate_report".into(), serde_json::json!({}));
    }

    // Default: health check
    ("system_health".into(), serde_json::json!({}))
}

// ==================== Response Generation ====================

fn generate_reply(intent: &str, tool_result: &serde_json::Value) -> String {
    match intent {
        "system_health" => {
            let status = tool_result["status"].as_str().unwrap_or("unknown");
            let pairs = tool_result["active_pairs"].as_u64().unwrap_or(0);
            let tenants = tool_result["tenants"].as_u64().unwrap_or(0);
            format!("✅ النظام شغال بكامل طاقته.\n• الحالة: {}\n• الأزواج النشطة: {}\n• المستأجرين: {}\n• الوقت: {}",
                status, pairs, tenants, tool_result["timestamp"].as_str().unwrap_or("—"))
        }
        "generate_marketing" => {
            let content = tool_result["content"].as_str().unwrap_or("");
            let platform = tool_result["platform"].as_str().unwrap_or("twitter");
            format!("📢 محتوى تسويقي لـ {}:\n\n{}", platform, content)
        }
        "trading_stats" => {
            let orders = tool_result["total_orders"].as_u64().unwrap_or(0);
            let trades = tool_result["total_trades"].as_u64().unwrap_or(0);
            let tenants = tool_result["active_tenants"].as_u64().unwrap_or(0);
            format!("📊 إحصائيات التداول:\n• إجمالي الأوامر: {}\n• إجمالي الصفقات: {}\n• المستأجرين النشطين: {}",
                orders, trades, tenants)
        }
        "compliance_report" => {
            let sars = tool_result["sars_filed"].as_u64().unwrap_or(0);
            format!("📋 تقرير الامتثال:\n• إجمالي SARs: {}",
                sars)
        }
        "market_overview" => {
            let markets = tool_result["markets"].as_array().map(|a| a.len()).unwrap_or(0);
            format!("📈 الأسواق المتاحة: {} زوج\n\n{}",
                markets,
                tool_result["markets"].as_array().map(|arr| {
                    arr.iter().map(|m| {
                        let pair = m["pair"].as_str().unwrap_or("—");
                        format!("• {}: متاح للتداول", pair)
                    }).collect::<Vec<_>>().join("\n")
                }).unwrap_or_default())
        }
        "generate_report" => {
            let summary = &tool_result["summary"];
            format!("📑 التقرير الشامل:\n\n• حالة النظام: {}\n• إجمالي المستخدمين: {}\n• المستخدمين النشطين: {}\n• إجمالي الأوامر: {}\n• إجمالي الصفقات: {}\n• SARs المرفوعة: {}",
                summary["system"].as_str().unwrap_or("—"),
                summary["total_users"].as_u64().unwrap_or(0),
                summary["active_users"].as_u64().unwrap_or(0),
                summary["total_orders"].as_u64().unwrap_or(0),
                summary["total_trades"].as_u64().unwrap_or(0),
                summary["sars_filed"].as_u64().unwrap_or(0))
        }
        "user_info" => {
            if tool_result.get("error").is_some() {
                "❌ المستخدم غير موجود.".to_string()
            } else {
                format!("👤 معلومات المستخدم:\n• المعرف: {}\n• الاسم: {}\n• المستوى: {}\n• الأوامر: {}\n• الصفقات: {}",
                    tool_result["tenant_id"].as_str().unwrap_or("—"),
                    tool_result["name"].as_str().unwrap_or("—"),
                    tool_result["tier"].as_str().unwrap_or("—"),
                    tool_result["total_orders"].as_u64().unwrap_or(0),
                    tool_result["total_trades"].as_u64().unwrap_or(0))
            }
        }
        "execute_trade" | "manage_user" | "broadcast_message" => {
            tool_result["message"].as_str().unwrap_or("❌ العملية غير متاحة من الشات.").to_string()
        }
        _ => {
            "✅ النظام جاهز. كيف أقدر أساعدك؟".to_string()
        }
    }
}

// ==================== AiAgent ====================

fn call_deepseek(api_key: &str, messages: &[ChatMessage]) -> Result<String, String> {
    let model = std::env::var("DEEPSEEK_MODEL").unwrap_or_else(|_| "deepseek-r1".into());
    let api_url = std::env::var("DEEPSEEK_API_URL").unwrap_or_else(|_| "https://api.deepseek.com/v1/chat/completions".into());

    let mut msgs = vec![
        serde_json::json!({"role": "system", "content": SYSTEM_PROMPT})
    ];
    for m in messages {
        msgs.push(serde_json::json!({"role": m.role, "content": m.content}));
    }

    let body = serde_json::json!({
        "model": model,
        "messages": msgs,
        "temperature": 0.7,
        "max_tokens": 4096,
    });

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| format!("client: {}", e))?;

    let resp = client.post(&api_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .map_err(|e| format!("request: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        return Err(format!("DeepSeek API {}: {}", status, text));
    }

    let json: serde_json::Value = resp.json().map_err(|e| format!("parse: {}", e))?;
    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| "no content in response".to_string())?
        .to_string();

    Ok(content)
}

pub struct AiAgent {
    pub config: Mutex<AiConfig>,
    pub sessions: Mutex<std::collections::HashMap<String, Vec<ChatMessage>>>,
    pub books: Arc<OrderBookManager>,
    pub compliance: Arc<ComplianceGateway>,
    pub orchestrator: Arc<CloudOrchestrator>,
}

impl AiAgent {
    pub fn new(
        books: Arc<OrderBookManager>,
        compliance: Arc<ComplianceGateway>,
        orchestrator: Arc<CloudOrchestrator>,
    ) -> Self {
        Self {
            config: Mutex::new(AiConfig::default()),
            sessions: Mutex::new(std::collections::HashMap::new()),
            books,
            compliance,
            orchestrator,
        }
    }

    pub fn chat(&self, req: ChatRequest) -> ChatResponse {
        let session_id = req.session_id.unwrap_or_else(|| Uuid::new_v4().to_string());

        let mut sessions = self.sessions.lock();
        let history = sessions.entry(session_id.clone()).or_insert_with(Vec::new);

        history.push(ChatMessage {
            role: "user".into(),
            content: req.message.clone(),
            tool_calls: None,
            name: None,
        });

        if history.len() > 20 {
            history.drain(0..history.len() - 20);
        }

        let cfg = self.config.lock().clone();
        let (intent, args) = classify_intent(&req.message);
        let ctx = ToolContext { books: &self.books, compliance: &self.compliance, orchestrator: &self.orchestrator };
        let tool_result = match execute_tool(&intent, &args, &ctx) {
            Ok(r) => r,
            Err(e) => serde_json::json!({"error": e}),
        };
        let tool_calls = vec![ToolCallDisplay {
            tool: intent.clone(),
            description: AiTool::name_from_str(&intent).map(|t| t.description()).unwrap_or("").to_string(),
            result: tool_result.clone(),
        }];

        let reply = if cfg.api_key_configured {
            let api_key = std::env::var("DEEPSEEK_API_KEY").unwrap_or_default();
            match call_deepseek(&api_key, history) {
                Ok(r) => r,
                Err(_) => generate_reply(&intent, &tool_result),
            }
        } else {
            generate_reply(&intent, &tool_result)
        };

        history.push(ChatMessage {
            role: "assistant".into(),
            content: reply.clone(),
            tool_calls: None,
            name: None,
        });

        info!(target: "ai_agent", session=%session_id, intent=%intent, llm=%cfg.api_key_configured, "AI chat processed");

        ChatResponse { reply, tool_calls, session_id }
    }

    pub fn config(&self) -> AiConfig {
        self.config.lock().clone()
    }

    pub fn update_config(&self, cfg: AiConfig) {
        *self.config.lock() = cfg;
        info!(target: "ai_agent", "Configuration updated");
    }
}

impl AiTool {
    fn name_from_str(s: &str) -> Option<Self> {
        match s {
            "system_health" => Some(AiTool::SystemHealth),
            "user_info" => Some(AiTool::UserInfo),
            "trading_stats" => Some(AiTool::TradingStats),
            "compliance_report" => Some(AiTool::ComplianceReport),
            "generate_marketing" => Some(AiTool::GenerateMarketing),
            "market_overview" => Some(AiTool::MarketOverview),
            "execute_trade" => Some(AiTool::ExecuteTrade),
            "manage_user" => Some(AiTool::ManageUser),
            "generate_report" => Some(AiTool::GenerateReport),
            "kyc_status" => Some(AiTool::KYCStatus),
            "broadcast_message" => Some(AiTool::BroadcastMessage),
            _ => None,
        }
    }
}
