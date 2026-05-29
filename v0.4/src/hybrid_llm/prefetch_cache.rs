use crate::config::Config;
use crate::store::KvStore;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct PrefetchCache {
    config: Arc<Config>,
    kvstore: Arc<KvStore>,
}

impl PrefetchCache {
    pub fn new(config: &Config, kvstore: Arc<KvStore>) -> Self {
        Self {
            config: Arc::new(config.clone()),
            kvstore,
        }
    }

    pub async fn lookup(&self, _prompt: &str) -> Option<String> {
        // Stub: return None (no cached response)
        None
    }

    pub async fn store(&self, _prompt: &str, _response: &str) {
        // Stub: do nothing
    }
}