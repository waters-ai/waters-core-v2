use crate::autonomy::AutonomyLevel;

/// SwitchProtocol decides whether to use local edge model, remote LLM, or a queue based on autonomy level.
pub struct SwitchProtocol;

impl SwitchProtocol {
    pub fn new() -> Self {
        Self
    }

    /// Determine the query mode based on autonomy level.
    pub fn resolve(&self, level: AutonomyLevel) -> super::edge_engine::QueryMode {
        match level {
            // L0: Use remote LLM (with validation by edge engine)
            AutonomyLevel::L0 => super::edge_engine::QueryMode::RemoteOnly,
            // L1: Use remote LLM (Ollama local) with edge as fallback
            AutonomyLevel::L1 => super::edge_engine::QueryMode::RemoteOnly,
            // L2: Use edge for simple queries, otherwise queue for sync
            AutonomyLevel::L2 => super::edge_engine::QueryMode::Auto,
            // L3: Use only edge engine
            AutonomyLevel::L3 => super::edge_engine::QueryMode::LocalOnly,
            // L4: Only SOS/beacon (safe mode) - we treat as local only for now
            AutonomyLevel::L4 => super::edge_engine::QueryMode::LocalOnly,
        }
    }
}