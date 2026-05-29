use crate::config::Config;
use crate::store::KvStore;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct SyncQueue {
    config: Arc<Config>,
    kvstore: Arc<KvStore>,
}

impl SyncQueue {
    pub fn new(config: &Config, kvstore: Arc<KvStore>) -> Self {
        Self {
            config: Arc::new(config.clone()),
            kvstore,
        }
    }

    // Stub methods
    pub async fn push(&self, _item: String) {
        // Do nothing
    }

    pub async fn pop(&self) -> Option<String> {
        None
    }
}