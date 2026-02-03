//! Agent Infrastructure
//!
//! Core infrastructure for building LLM-powered agents.
//!
//! This module provides:
//! - AgentCore - Complete working agent infrastructure
//! - Message types for Frontend-Controller communication
//! - Input routing between Frontend and controller
//! - Logging infrastructure
//! - Configuration management with trait-based customization
//!
//! # Quick Start (Headless)
//!
//! ```ignore
//! use agent_core_runtime::agent::{AgentConfig, AgentCore};
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
//!
//!     // Get channels for custom frontend integration
//!     let tx = core.to_controller_tx();
//!     let rx = core.take_from_controller_rx();
//!
//!     // Create a session and interact programmatically
//!     let (session_id, model, _) = core.create_initial_session()?;
//!     // ... implement your own event loop
//!
//!     core.shutdown();
//!     Ok(())
//! }
//! ```

mod config;
mod core;
mod environment;
mod error;
mod logger;
mod messages;
mod providers;
mod router;

pub use config::{load_config, AgentConfig, ConfigError, ConfigFile, LLMRegistry, ProviderConfig};
pub use environment::EnvironmentContext;
pub use providers::{get_provider_info, is_known_provider, list_providers, ProviderInfo};
pub use error::AgentError;
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
