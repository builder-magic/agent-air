//! LLM session management with automatic context compaction.

mod compactor;
mod config;
mod convert;
mod manager;
mod session;

pub use compactor::{
    AsyncCompactor, CompactionError, CompactionResult, Compactor, CompactorConfigError,
    LLMCompactor, LLMCompactorConfig, ThresholdCompactor, ToolCompaction,
};
pub use config::{CompactionConfig, CompactorType, LLMProvider, LLMSessionConfig};
pub use manager::LLMSessionManager;
pub use session::{CompactResult, LLMSession, SessionStatus, TokenUsage};
