//! LLM session management with automatic context compaction.

mod compactor;
mod config;
mod convert;
mod llm_session;
mod manager;

pub use compactor::{
    AsyncCompactor, CompactionError, CompactionResult, Compactor, CompactorConfigError,
    LLMCompactor, LLMCompactorConfig, ThresholdCompactor, ToolCompaction,
};
pub use config::{CompactionConfig, CompactorType, LLMProvider, LLMSessionConfig};
pub use llm_session::{CompactResult, LLMSession, SessionStatus, TokenUsage};
pub use manager::LLMSessionManager;
