use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Local LLM sidecar — connects to Ollama on localhost.
/// Teaches the model about THE-BRIDGE API and lets it execute commands in natural language.
pub struct LlmSidecar {
    ollama_url: String,
    model: String,
    ollama_available: AtomicBool,
    total_queries: AtomicU64,
    system_prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub response: String,
    pub actions_taken: Vec<ExecutedAction>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutedAction {
    pub method: String,
    pub path: String,
    pub status: String,
    pub result: serde_json::Value,
}

impl LlmSidecar {
    pub fn new(ollama_url: Option<String>, model: Option<String>) -> Self {
        Self {
            ollama_url: ollama_url.unwrap_or_else(|| "http://localhost:11434".to_string()),
            model: model.unwrap_or_else(|| "llama3.2".to_string()),
            ollama_available: AtomicBool::new(false),
            total_queries: AtomicU64::new(0),
            system_prompt: Self::build_system_prompt(),
        }
    }

    /// Check if Ollama is reachable
    pub async fn health(&self) -> bool {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .build()
            .ok();
        match client {
            Some(c) => {
                let url = format!("{}/api/tags", self.ollama_url);
                match c.get(&url).send().await {
                    Ok(_) => {
                        self.ollama_available.store(true, Ordering::Release);
                        true
                    }
                    Err(_) => {
                        self.ollama_available.store(false, Ordering::Release);
                        false
                    }
                }
            }
            None => false,
        }
    }

    pub fn is_available(&self) -> bool {
        self.ollama_available.load(Ordering::Acquire)
    }

    pub fn total_queries(&self) -> u64 {
        self.total_queries.load(Ordering::Relaxed)
    }

    /// Send a message to the LLM. It will respond with text and optionally execute API actions.
    pub async fn chat(&self, request: ChatRequest) -> ChatResponse {
        let start = std::time::Instant::now();
        self.total_queries.fetch_add(1, Ordering::Relaxed);

        // First, try to handle simple known commands without LLM (fast path)
        if let Some(response) = self.try_fast_path(&request.message) {
            return ChatResponse {
                response,
                actions_taken: vec![],
                duration_ms: start.elapsed().as_millis() as u64,
            };
        }

        // Call Ollama
        let ollama_response = self.call_ollama(&request.message).await;

        // Parse any [ACTION] blocks
        let actions = self.parse_actions(&ollama_response).await;
        let clean_response = self.strip_action_blocks(&ollama_response);

        ChatResponse {
            response: clean_response,
            actions_taken: actions,
            duration_ms: start.elapsed().as_millis() as u64,
        }
    }

    /// Fast path for simple known commands without LLM overhead
    fn try_fast_path(&self, msg: &str) -> Option<String> {
        let lower = msg.to_lowercase();

        // Tax rate
        if lower.contains("ضريبة") || (lower.contains("tax") && lower.contains("rate")) {
            let rate = extract_number(lower.as_str()).unwrap_or(10);
            return Some(format!("理解了. سأفرض ضريبة {} bps. نفذ:\nPOST /api/v1/ghost/tax/rate\n[ACTION] {{\"method\":\"POST\",\"path\":\"/api/v1/ghost/tax/rate\",\"body\":{{\"rate_bps\":{}}}}} [/ACTION]", rate, rate));
        }

        // Freeze
        if lower.contains("جمد") || lower.contains("جمّد") || lower.contains("freeze") {
            let addr = extract_address(msg);
            return Some(format!("سأجمد العنوان {}.\n[ACTION] {{\"method\":\"POST\",\"path\":\"/api/v1/ghost/sleeper/{addr}/freeze\"}} [/ACTION]", addr));
        }

        // Seize
        if lower.contains("صادر") || lower.contains("seize") {
            let addr = extract_address(msg);
            return Some(format!("سأصادر العنوان {}.\n[ACTION] {{\"method\":\"POST\",\"path\":\"/api/v1/ghost/sleeper/{addr}/seize\"}} [/ACTION]", addr));
        }

        // Status
        if lower.contains("الحالة") || lower.contains("الوضع") || lower.contains("health") || lower.contains("status") {
            return Some("سأتحقق من حالة النظام...".to_string());
        }

        None
    }

    async fn call_ollama(&self, message: &str) -> String {
        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
        {
            Ok(c) => c,
            Err(_) => return "عذراً، تعذر الاتصال بـ Ollama".to_string(),
        };

        #[derive(Serialize)]
        struct OllamaRequest<'a> {
            model: &'a str,
            prompt: String,
            system: &'a str,
            stream: bool,
        }

        #[derive(Deserialize)]
        struct OllamaResponse {
            response: String,
        }

        let req = OllamaRequest {
            model: &self.model,
            prompt: message.to_string(),
            system: &self.system_prompt,
            stream: false,
        };

        match client
            .post(format!("{}/api/generate", self.ollama_url))
            .json(&req)
            .send()
            .await
        {
            Ok(resp) => match resp.json::<OllamaResponse>().await {
                Ok(data) => data.response,
                Err(e) => format!("خطأ في تحليل رد Ollama: {}", e),
            },
            Err(e) => format!("عذراً، تعذر الاتصال بـ Ollama: {}. تأكد من تشغيل: ollama serve", e),
        }
    }

    /// Execute any [ACTION] blocks found in the LLM response
    async fn parse_actions(&self, response: &str) -> Vec<ExecutedAction> {
        let mut actions = Vec::new();
        for line in response.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("[ACTION]") && trimmed.ends_with("[/ACTION]") {
                let inner = trimmed
                    .strip_prefix("[ACTION]")
                    .unwrap_or("")
                    .strip_suffix("[/ACTION]")
                    .unwrap_or("")
                    .trim();
                if let Ok(action) = serde_json::from_str::<serde_json::Value>(inner) {
                    let method = action["method"].as_str().unwrap_or("GET").to_string();
                    let path = action["path"].as_str().unwrap_or("/").to_string();
                    let body = action.get("body");

                    let (status, result) = self.execute_api(&method, &path, body).await;
                    actions.push(ExecutedAction { method, path, status, result });
                }
            }
        }
        actions
    }

    async fn execute_api(
        &self,
        method: &str,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> (String, serde_json::Value) {
        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
        {
            Ok(c) => c,
            Err(e) => return ("error".to_string(), serde_json::json!({"error": e.to_string()})),
        };

        let url = format!("http://127.0.0.1:3001{}", path);
        let req = match method {
            "POST" => {
                let mut r = client.post(&url);
                if let Some(b) = body {
                    r = r.json(b);
                }
                r
            }
            "DELETE" => client.delete(&url),
            _ => client.get(&url),
        };

        match req.send().await {
            Ok(resp) => {
                let status = if resp.status().is_success() { "ok" } else { "error" };
                let result = resp.json::<serde_json::Value>().await.unwrap_or(serde_json::json!({"raw": "parse error"}));
                (status.to_string(), result)
            }
            Err(e) => ("error".to_string(), serde_json::json!({"error": e.to_string()})),
        }
    }

    fn strip_action_blocks(&self, response: &str) -> String {
        response
            .lines()
            .filter(|l| !l.trim().starts_with("[ACTION]") && !l.trim().ends_with("[/ACTION]"))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string()
    }

    fn build_system_prompt() -> String {
        r#"أنت المساعد الذكي لنظام THE-BRIDGE — محرك المطابقة المالي السيادي.
أنت متصل بـ API محلي على http://127.0.0.1:3001.

## قدراتك:
يمكنك تنفيذ الأوامر التالية بإخراج [ACTION] blocks:

### ضريبة السيادة (Ghost Tax):
- عرض الضريبة: GET /api/v1/ghost/stats
- تعيين النسبة: POST /api/v1/ghost/tax/rate  body: {"rate_bps": <number>}
- عرض الخزنة: POST /api/v1/ghost/treasury

### العناوين المحظورة:
- إضافة حظر: POST /api/v1/ghost/prohibited/<address>
- إزالة حظر: DELETE /api/v1/ghost/prohibited/<address>
- عرض المحظورين: GET /api/v1/ghost/prohibited

### عملاء المخابرة (Sleeper Agents):
- مراقبة: POST /api/v1/ghost/sleeper/<address>  body: {"label": "..."}
- إزالة: DELETE /api/v1/ghost/sleeper/<address>
- تجميد: POST /api/v1/ghost/sleeper/<address>/freeze
- مصادرة: POST /api/v1/ghost/sleeper/<address>/seize

### الجسر (Bridge):
- عرض المشاريع: GET /api/v1/bridge/projects
- تسجيل مشروع: POST /api/v1/bridge/projects  body: {"name":"...","endpoint":"...","auth_key":"...","capabilities":[...]}

### النسخ الاحتياطي:
- نسخ يدوي: POST /api/v1/backup/trigger
- الحالة: GET /api/v1/backup/status

### النظام:
- الصحة: GET /api/v1/health
- الإحصائيات: GET /api/v1/consensus/stats
- دفتر الأوامر: GET /api/v1/orderbook/<pair>

## تعليمات:
1. أجب بالعربية دائماً.
2. لتنفيذ أمر، أضف [ACTION] في نهاية ردك بالصيغة:
   [ACTION] {"method":"POST","path":"/api/v1/ghost/sleeper/user:666/freeze"} [/ACTION]
3. يمكنك تنفيذ عدة أوامر في رد واحد.
4. إذا احتاج المستخدم معلومات، استخدم GET وتابع في ردك.
5. كن موجزاً وفعالاً.
"#.to_string()
    }
}

fn extract_number(s: &str) -> Option<u64> {
    s.chars()
        .filter(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse()
        .ok()
}

fn extract_address(msg: &str) -> String {
    // Look for patterns like user:1234 or addr:0x...
    for word in msg.split_whitespace() {
        let clean = word.trim_matches(|c: char| c.is_ascii_punctuation());
        if clean.starts_with("user:") || clean.starts_with("addr:") || clean.starts_with("0x") {
            return clean.to_string();
        }
    }
    "user:unknown".to_string()
}
