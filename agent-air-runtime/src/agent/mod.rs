//! Agent Infrastructure
//!
//! Core infrastructure for building LLM-powered agents.
//!
//! This module provides:
//! - AgentAir - Complete working agent infrastructure
//! - Message types for Frontend-Controller communication
//! - Input routing between Frontend and controller
//! - Logging infrastructure
//! - Configuration management with trait-based customization
//!
//! # Quick Start (Headless)
//!
//! ```ignore
//! use agent_air_runtime::agent::{AgentConfig, AgentAir};
//!
//! struct MyConfig;
//! impl AgentConfig for MyConfig {
//!     fn state_dir(&self) -> &str { "~/.myagent" }
//!     fn default_system_prompt(&self) -> &str { "You are helpful." }
//!     fn log_prefix(&self) -> &str { "myagent" }
//!     fn name(&self) -> &str { "MyAgent" }
//! }
//!
//! fn main() -> std::io::Result<()> {
//!     let mut core = AgentAir::new(&MyConfig)?;
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
pub mod interface;
mod logger;
mod messages;
mod providers;
mod router;

pub use config::{
    AgentConfig, ConfigError, ConfigFile, EnvLoading, LLMRegistry, ProviderConfig, SimpleConfig,
    load_config,
};

// Re-export commonly used interface types at agent level for convenience
pub use core::{
    AgentAir, FromControllerRx, FromControllerTx, ToControllerRx, ToControllerTx,
    convert_controller_event_to_ui_message,
};
pub use environment::EnvironmentContext;
pub use error::AgentError;
pub use interface::{
    // Policy types
    AutoApprovePolicy,
    // Sink types
    ChannelEventSink,
    // Source types
    ChannelInputSource,
    DenyAllPolicy,
    EventSink,
    InputSource,
    InteractivePolicy,
    PermissionPolicy,
    PolicyDecision,
    SendError,
    SimpleEventSink,
};
pub use logger::Logger;
pub use messages::UiMessage;
pub use messages::channels::{DEFAULT_CHANNEL_SIZE, create_channels};
pub use providers::{ProviderInfo, get_provider_info, is_known_provider, list_providers};
pub use router::InputRouter;

// Re-export database types when the db feature is enabled
#[cfg(feature = "db")]
pub use crate::db::{AgentDatabase, DbConfig, DbError, Store};

// Re-export common types from llm-controller-rs that agents typically need
pub use crate::controller::{
    ControllerEvent, ControllerInputPayload, LLMController, LLMSessionConfig, PermissionRegistry,
    ToolResultStatus, TurnId, UserInteractionRegistry,
};
