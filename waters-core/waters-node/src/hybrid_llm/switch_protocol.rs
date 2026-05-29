use crate::autonomy::AutonomyLevel;
use crate::hybrid_llm::edge_engine::QueryMode;

pub struct SwitchProtocol;

impl SwitchProtocol {
    pub fn new() -> Self {
        Self
    }

    pub fn resolve(&self, level: AutonomyLevel) -> QueryMode {
        // Simple mapping: low autonomy -> edge, high -> remote, etc.
        match level {
            AutonomyLevel::L0 | AutonomyLevel::L1 => QueryMode::LocalOnly,
            AutonomyLevel::L2 | AutonomyLevel::L3 => QueryMode::Auto,
            AutonomyLevel::L4 => QueryMode::RemoteOnly,
            // For now, treat L4 as RemoteOnly; Distributed not implemented
            // AutonomyLevel::L5 => QueryMode::Distributed,
        }
    }
}
