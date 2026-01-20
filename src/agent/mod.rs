//! Agent Infrastructure
//!
//! Core infrastructure for building LLM-powered agents.
//!
//! This module provides:
//! - [`AgentCore`] - Complete working agent infrastructure
//! - Message types for TUI-Controller communication
//! - Input routing between TUI and controller
//! - Logging infrastructure
//! - Configuration management with trait-based customization
//!
//! # Quick Start
//!
//! ```ignore
//! use agent_core::agent::{AgentConfig, AgentCore};
//!
//! struct MyConfig;
//! impl AgentConfig for MyConfig {
//!     fn config_path(&self) -> &str { ".myagent/config.yaml" }
//!     fn default_system_prompt(&self) -> &str { "You are helpful." }
//!     fn log_prefix(&self) -> &str { "myagent" }
//!     fn name(&self) -> &str { "MyAgent" }
//! }
//!
//! fn main() -> std::io::Result<()> {
//!     let mut core = AgentCore::new(&MyConfig)?;
//!     core.start_background_tasks();
//!     // Wire up your TUI and run
//!     Ok(())
//! }
//! ```

mod config;
mod core;
mod logger;
mod messages;
mod router;

pub use config::{load_config, AgentConfig, ConfigError, ConfigFile, LLMRegistry, ProviderConfig};
pub use core::{
    convert_controller_event_to_ui_message, AgentCore, FromControllerRx, FromControllerTx,
    ToControllerRx, ToControllerTx,
};
pub use logger::Logger;
pub use messages::channels::{create_channels, DEFAULT_CHANNEL_SIZE};
pub use messages::UiMessage;
pub use router::InputRouter;

// Re-export common types from llm-controller-rs that agents typically need
pub use crate::controller::{
    ControllerEvent, ControllerInputPayload, LLMController, LLMSessionConfig, PermissionRegistry,
    ToolResultStatus, TurnId, UserInteractionRegistry,
};
