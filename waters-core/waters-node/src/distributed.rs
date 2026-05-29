use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

use crate::config::Config;
use crate::store::KvStore;

/// Топология доменов для маршрутизации запросов в распределенной сети.
#[derive(Debug, Clone)]
pub struct DomainRouter {
    /// Карта доменов к их шлюзам (узлам, которые могут обрабатывать запросы для этого домена).
    domain_gateways: Arc<Mutex<HashMap<String, Vec<String>>>>,
}

impl DomainRouter {
    pub fn new() -> Self {
        Self {
            domain_gateways: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Зарегистрировать шлюз для домена.
    pub async fn register_gateway(&self, domain: &str, gateway_node_id: String) {
        let mut gateways = self.domain_gateways.lock().await;
        gateways
            .entry(domain.to_string())
            .or_default()
            .push(gateway_node_id);
        info!(domain, gateway_node_id, "Registered gateway for domain");
    }

    /// Получить список шлюзов для домена.
    pub async fn get_gateways(&self, domain: &str) -> Vec<String> {
        let gateways = self.domain_gateways.lock().await;
        gateways.get(domain).cloned().unwrap_or_default()
    }
}

/// Кеш для распределенных данных (например, токенизированных промптов или ответов).
/// Использует локальное хранилище (RocksDB или аналогичное) для быстрого доступа.
#[derive(Debug)]
pub struct DistributedCache {
    /// Ссылка на хранилище ключ-значение (используем существующий KvStore из waters-node).
    kvstore: Arc<KvStore>,
}

impl DistributedCache {
    pub fn new(kvstore: Arc<KvStore>) -> Self {
        Self { kvstore }
    }

    /// Сохранить значение в кеше по ключу.
    pub async fn set(&self, key: &str, value: Vec<u8>) -> anyhow::Result<()> {
        self.kvstore.set(key, value).await?;
        Ok(())
    }

    /// Получить значение из кеша по ключу.
    pub async fn get(&self, key: &str) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(self.kvstore.get(key).await?)
    }

    /// Удалить значение из кеша по ключу.
    pub async fn delete(&self, key: &str) -> anyhow::Result<()> {
        self.kvstore.delete(key).await?;
        Ok(())
    }
}

/// Пул токенов для управления лимитами использования LLM в распределенной сети.
/// Каждый узел имеет определенное количество токенов, которые можно тратить на запросы.
#[derive(Debug)]
pub struct TokenPool {
    /// Максимальное количество токенов, доступных в пуле.
    max_tokens: u64,
    /// Текущее количество доступных токенов.
    available_tokens: Arc<tokio::sync::Mutex<u64>>,
}

impl TokenPool {
    pub fn new(max_tokens: u64) -> Self {
        Self {
            max_tokens,
            available_tokens: Arc::new(tokio::sync::Mutex::new(max_tokens)),
        }
    }

    /// Попытаться взять указанное количество токенов из пула.
    /// Возвращает true, если токены были успешно взяты, false иначе.
    pub async fn acquire(&self, tokens: u64) -> bool {
        let mut available = self.available_tokens.lock().await;
        if *available >= tokens {
            *available -= tokens;
            true
        } else {
            false
        }
    }

    /// Вернуть токены обратно в пул.
    pub async fn release(&self, tokens: u64) {
        let mut available = self.available_tokens.lock().await;
        *available = (*available + tokens).min(self.max_tokens);
    }

    /// Получить текущее количество доступных токенов.
    pub async fn available(&self) -> u64 {
        *self.available_tokens.lock().await
    }
}