use crate::config::Config;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Query modes for the edge engine.
#[derive(Debug, Clone, Copy)]
pub enum QueryMode {
    LocalOnly,
    RemoteOnly,
    Auto,
    Distributed,
}

pub struct EdgeEngine {
    config: Arc<Config>,
}

impl EdgeEngine {
    pub fn new(config: &Config) -> Self {
        Self {
            config: Arc::new(config.clone()),
        }
    }

    pub async fn generate(&self, _prompt: &str) -> String {
        // Stub: return a default response
        "EdgeEngine response (stub)".to_string()
    }

    pub fn is_simple_query(&self, _prompt: &str) -> bool {
        // Stub: consider all queries simple for edge
        true
    }
}