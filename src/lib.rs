//! Agent Core
//!
//! A Rust Framework for building TUI Agents powered by large language models.
//!
//! This is a meta-crate that re-exports from:
//! - `agent-core-runtime` - Core runtime (always included)
//! - `agent-core-tui` - TUI frontend (optional, enabled by default)
//!
//! ## Features
//!
//! - `tui` (default) - Include the TUI frontend
//!
//! ## Quick Start (with TUI)
//!
//! ```ignore
//! use agent_core::agent::{AgentConfig, AgentCore};
//! use agent_core::tui::AgentCoreExt;
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
//!     let agent = AgentCore::new(&MyConfig)?;
//!     agent.into_tui().run()
//! }
//! ```
//!
//! ## Headless Usage (without TUI)
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

// Re-export everything from the runtime crate
pub use agent_core_runtime::*;

// Re-export the TUI crate when the feature is enabled
#[cfg(feature = "tui")]
pub mod tui {
    //! TUI frontend for agent-core.
    //!
    //! This module provides a ratatui-based terminal interface for agents.
    //! Use `AgentCoreExt::into_tui()` to convert an `AgentCore` into a `TuiRunner`.

    pub use agent_core_tui::*;
}
