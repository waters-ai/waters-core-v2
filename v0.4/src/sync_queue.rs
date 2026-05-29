use std::io::Write;
use std::path::PathBuf;
use tracing::info;

use crate::config::Config;
use crate::store::KvStore;
use serde::{Deserialize, Serialize};

/// Структура для хранения запроса и ответа от edge-модели, предназначенного для синхронизации.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QueuedQuery {
    pub prompt: String,
    pub edge_response: String,
    pub timestamp: String,
}

/// Очередь синхронизации для хранения пар (prompt, edge_response) при работе в режиме L2-L3.
/// При восстановлении связи эти данные отправляются на удаленную LLM для сверки.
pub struct SyncQueue {
    queue_path: PathBuf,
}

impl SyncQueue {
    pub fn new(base_path: &std::path::Path, _config: &Config, _kvstore: Arc<KvStore>) -> Self {
        // We store the sync queue in a separate file, e.g., ".waters/sync_queue.jsonl"
        let p = base_path.join("sync_queue.jsonl");
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        SyncQueue { queue_path: p }
    }

    /// Добавить пары (prompt, edge_response) в очередь синхронизации.
    pub fn enqueue(&self, prompt: &str, edge_response: &str) -> anyhow::Result<()> {
        let entry = QueuedQuery {
            prompt: prompt.to_string(),
            edge_response: edge_response.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        let line = serde_json::to_string(&entry)?;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.queue_path)
            .map_err(|e| anyhow::anyhow!("Failed to open sync queue: {}", e))?
            .write_fmt(format_args!("{}\n", line))?;

        info!("Sync queue enqueued: {} bytes", line.len());
        Ok(())
    }

    /// Прочитать и очистить очередь синхронизации.
    /// Возвращает векторизацию записей для отправки на удаленную LLM.
    pub fn flush(&self) -> anyhow::Result<Vec<QueuedQuery>> {
        if !self.queue_path.exists() {
            return Ok(Vec::new());
        }
        let content = std::fs::read_to_string(&self.queue_path)?;
        let mut entries: Vec<QueuedQuery> = content
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        // Clear the queue after reading
        std::fs::write(&self.queue_path, "")?;
        info!("Sync queue flushed: {} entries", entries.len());
        Ok(entries)
    }

    /// Получить текущую длину очереди (без очистки).
    pub fn len(&self) -> anyhow::Result<usize> {
        if !self.queue_path.exists() {
            return Ok(0);
        }
        let content = std::fs::read_to_string(&self.queue_path)?;
        let count = content.lines().count();
        Ok(count)
    }
}