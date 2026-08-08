use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiFlavor {
    Ollama,
    OpenAICompat,
}

#[allow(async_fn_in_trait)]
pub trait LlmProvider: Send + Sync {
    fn health(&self) -> Vec<String>;
    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> std::result::Result<CompletionResponse, ProviderError>;
    async fn chat(
        &self,
        messages: &[ChatMessage],
    ) -> std::result::Result<CompletionResponse, ProviderError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("transport error: {0}")]
    Transport(String),
    #[error("unavailable: {0}")]
    Unavailable(String),
    #[error("model not found: {0}")]
    ModelNotFound(String),
    #[error("status {status}: {body}")]
    Status { status: u16, body: String },
    #[error("timeout")]
    Timeout,
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

impl From<reqwest::Error> for ProviderError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            ProviderError::Timeout
        } else if e.is_status() {
            ProviderError::Status {
                status: e.status().map(|s| s.as_u16()).unwrap_or(0),
                body: e.to_string(),
            }
        } else {
            ProviderError::Transport(e.to_string())
        }
    }
}

#[derive(Debug, Clone)]
pub struct OllamaConfig {
    pub base_url: String,
    pub model: String,
    pub api_flavor: ApiFlavor,
    pub timeout_secs: u64,
    pub system_prompt: String,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:11434".to_string(),
            model: "deepseek-r1".to_string(),
            api_flavor: ApiFlavor::Ollama,
            timeout_secs: 30,
            system_prompt: "You are THE-BRIDGE AI CEO, an autonomous liquidity and risk engine.".to_string(),
        }
    }
}

pub struct OllamaClient {
    config: OllamaConfig,
    client: reqwest::Client,
}

impl OllamaClient {
    pub fn new(config: OllamaConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .unwrap_or_default();
        Self { config, client }
    }

    pub async fn health(&self) -> bool {
        let url = match self.config.api_flavor {
            ApiFlavor::Ollama => format!("{}/api/tags", self.config.base_url),
            ApiFlavor::OpenAICompat => format!("{}/v1/models", self.config.base_url),
        };
        self.client
            .get(&url)
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    pub fn health_status(&self) -> HealthStatus {
        let endpoints = match self.config.api_flavor {
            ApiFlavor::Ollama => vec![
                format!("{}/api/tags", self.config.base_url),
                format!("{}/api/generate", self.config.base_url),
            ],
            ApiFlavor::OpenAICompat => vec![
                format!("{}/v1/models", self.config.base_url),
                format!("{}/v1/completions", self.config.base_url),
            ],
        };
        HealthStatus {
            available: false,
            endpoints,
            model: self.config.model.clone(),
        }
    }

    pub async fn complete(
        &self,
        request: CompletionRequest,
    ) -> std::result::Result<CompletionResponse, ProviderError> {
        let prompt = format!("{}\n\n{}", self.config.system_prompt, request.prompt);
        match self.config.api_flavor {
            ApiFlavor::Ollama => self.call_ollama(&prompt).await,
            ApiFlavor::OpenAICompat => self.call_openai(&prompt).await,
        }
    }

    pub async fn chat(
        &self,
        messages: &[ChatMessage],
    ) -> std::result::Result<CompletionResponse, ProviderError> {
        match self.config.api_flavor {
            ApiFlavor::Ollama => {
                let prompt = messages
                    .iter()
                    .map(|m| format!("{}: {}", m.role, m.content))
                    .collect::<Vec<_>>()
                    .join("\n");
                let full_prompt = format!("{}\n\n{}", self.config.system_prompt, prompt);
                self.call_ollama(&full_prompt).await
            }
            ApiFlavor::OpenAICompat => self.call_openai_chat(messages).await,
        }
    }

    async fn call_ollama(&self, prompt: &str) -> std::result::Result<CompletionResponse, ProviderError> {
        let url = format!("{}/api/generate", self.config.base_url);
        #[derive(serde::Serialize)]
        struct Req<'a> {
            model: &'a str,
            prompt: &'a str,
            system: &'a str,
            stream: bool,
            options: serde_json::Value,
        }
        let req = Req {
            model: &self.config.model,
            prompt,
            system: &self.config.system_prompt,
            stream: false,
            options: serde_json::json!({
                "temperature": 0.7,
                "top_p": 0.9,
                "num_ctx": 16384,
            }),
        };
        let resp = self
            .client
            .post(&url)
            .json(&req)
            .send()
            .await
            .map_err(|e| ProviderError::Transport(e.to_string()))?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Status { status, body });
        }
        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(ProviderError::from)?;
        let content = data["response"].as_str().ok_or_else(|| {
            ProviderError::Status {
                status: 500,
                body: "missing response field".to_string(),
            }
        })?;
        Ok(CompletionResponse {
            content: content.to_string(),
            model: self.config.model.clone(),
            tokens: data["eval_count"].as_u64().unwrap_or(0) as u32,
        })
    }

    async fn call_openai(&self, prompt: &str) -> std::result::Result<CompletionResponse, ProviderError> {
        let url = format!("{}/v1/completions", self.config.base_url);
        let req = serde_json::json!({
            "model": self.config.model,
            "messages": [
                {"role": "system", "content": self.config.system_prompt},
                {"role": "user", "content": prompt},
            ],
            "temperature": 0.7,
            "top_p": 0.9,
            "max_tokens": 16384,
        });
        let resp = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&req)
            .send()
            .await
            .map_err(|e| ProviderError::Transport(e.to_string()))?;
        self.handle_openai_response(resp).await
    }

    async fn call_openai_chat(
        &self,
        messages: &[ChatMessage],
    ) -> std::result::Result<CompletionResponse, ProviderError> {
        let url = format!("{}/v1/chat/completions", self.config.base_url);
        let req_messages: Vec<serde_json::Value> = messages
            .iter()
            .map(|m| serde_json::json!({"role": m.role, "content": m.content}))
            .collect();
        let req = serde_json::json!({
            "model": self.config.model,
            "messages": req_messages,
            "temperature": 0.7,
            "top_p": 0.9,
            "max_tokens": 16384,
        });
        let resp = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&req)
            .send()
            .await
            .map_err(|e| ProviderError::Transport(e.to_string()))?;
        self.handle_openai_response(resp).await
    }

    async fn handle_openai_response(
        &self,
        resp: reqwest::Response,
    ) -> std::result::Result<CompletionResponse, ProviderError> {
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Status { status, body });
        }
        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(ProviderError::from)?;
        let content = data["choices"][0]["message"]["content"]
            .as_str()
            .or_else(|| data["choices"][0]["text"].as_str())
            .ok_or_else(|| ProviderError::Status {
                status: 500,
                body: "missing content in response".to_string(),
            })?;
        let tokens = data["usage"]["total_tokens"].as_u64().unwrap_or(0) as u32;
        Ok(CompletionResponse {
            content: content.to_string(),
            model: data["model"].as_str().unwrap_or(&self.config.model).to_string(),
            tokens,
        })
    }
}

impl LlmProvider for OllamaClient {
    fn health(&self) -> Vec<String> {
        self.health_status().endpoints
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> std::result::Result<CompletionResponse, ProviderError> {
        self.complete(request).await
    }

    async fn chat(
        &self,
        messages: &[ChatMessage],
    ) -> std::result::Result<CompletionResponse, ProviderError> {
        self.chat(messages).await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub prompt: String,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub top_p: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub content: String,
    pub model: String,
    pub tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub available: bool,
    pub model: String,
    pub endpoints: Vec<String>,
}
