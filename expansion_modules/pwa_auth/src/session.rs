use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub user_id: String,
    pub credential_id: String,
    pub created_at: DateTime<Utc>,
    pub last_active: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub active: bool,
}

impl Session {
    pub fn new(user_id: &str, credential_id: &str, ttl_seconds: u64) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: user_id.to_string(),
            credential_id: credential_id.to_string(),
            created_at: now,
            last_active: now,
            expires_at: now + Duration::seconds(ttl_seconds as i64),
            active: true,
        }
    }

    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    pub fn is_active(&self) -> bool {
        self.active && !self.is_expired()
    }
}

#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn create(&self, session: Session) -> Result<(), StoreError>;
    async fn get(&self, session_id: &str) -> Option<Session>;
    async fn destroy(&self, session_id: &str) -> bool;
    async fn refresh(&self, session_id: &str, ttl_seconds: u64) -> Option<Session>;
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("redis error: {0}")]
    Redis(String),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("not found")]
    NotFound,
    #[error("internal error: {0}")]
    Internal(String),
}

#[derive(Clone)]
pub struct InMemorySessionStore {
    sessions: Arc<RwLock<std::collections::HashMap<String, Session>>>,
    cleanup_interval_secs: u64,
}

impl InMemorySessionStore {
    pub fn new(cleanup_interval_secs: u64) -> Self {
        let store = Self {
            sessions: Arc::new(RwLock::new(std::collections::HashMap::new())),
            cleanup_interval_secs,
        };
        store.start_cleanup();
        store
    }

    fn start_cleanup(&self) {
        let sessions = self.sessions.clone();
        let interval_secs = self.cleanup_interval_secs;
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
            loop {
                ticker.tick().await;
                let now = Utc::now();
                sessions.write().await.retain(|_, s| s.expires_at > now);
            }
        });
    }
}

impl Default for InMemorySessionStore {
    fn default() -> Self {
        Self::new(300)
    }
}

#[async_trait]
impl SessionStore for InMemorySessionStore {
    async fn create(&self, session: Session) -> Result<(), StoreError> {
        self.sessions.write().await.insert(session.id.clone(), session);
        Ok(())
    }

    async fn get(&self, session_id: &str) -> Option<Session> {
        self.sessions
            .read()
            .await
            .get(session_id)
            .cloned()
            .filter(|s| s.is_active())
    }

    async fn destroy(&self, session_id: &str) -> bool {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            session.active = false;
            true
        } else {
            false
        }
    }

    async fn refresh(&self, session_id: &str, ttl_seconds: u64) -> Option<Session> {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            let now = Utc::now();
            session.expires_at = now + Duration::seconds(ttl_seconds as i64);
            session.last_active = now;
            Some(session.clone())
        } else {
            None
        }
    }
}
