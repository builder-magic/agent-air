//! Agent Infrastructure
//!
//! Core infrastructure for building LLM-powered agents.
//!
//! This module provides:
//! - Message types for TUI-Controller communication
//! - Input routing between TUI and controller
//! - Logging infrastructure
//! - Configuration management with trait-based customization
//! - Base agent trait for building custom agents

mod config;
mod logger;
mod messages;
mod router;

pub use config::{load_config, AgentConfig, ConfigError, ConfigFile, LLMRegistry, ProviderConfig};
pub use logger::Logger;
pub use messages::channels::{
    create_channels, FromControllerRx, FromControllerTx, ToControllerRx, ToControllerTx,
    DEFAULT_CHANNEL_SIZE,
};
pub use messages::UiMessage;
pub use router::InputRouter;

// Re-export common types from llm-controller-rs that agents typically need
pub use llm_controller_rs::{
    ControllerEvent, ControllerInputPayload, LLMController, LLMSessionConfig, PermissionRegistry,
    ToolResultStatus, TurnId, UserInteractionRegistry,
};
