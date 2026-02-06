//! Engine Interface Abstractions
//!
//! This module defines the contract between the agent engine and external
//! consumers (TUI, servers, CLI tools, etc.).
//!
//! # Architecture
//!
//! The interface is message-based and transport-agnostic:
//!
//! ```text
//! Consumer          Interface           Engine
//!    │                                    │
//!    │◄─────── EventSink ────────────────│  (events out)
//!    │                                    │
//!    │─────── InputSource ──────────────►│  (input in)
//!    │                                    │
//!    │◄────── PermissionRegistry ───────►│  (request/response)
//! ```
//!
//! # Components
//!
//! - [`EventSink`] - Receives events from the engine (text, tools, errors)
//! - [`InputSource`] - Provides input to the engine (user messages, commands)
//! - [`PermissionPolicy`] - Automatic handling of permission requests
//!
//! # Built-in Implementations
//!
//! - [`ChannelEventSink`] - Channel-backed sink (default, used by TUI)
//! - [`ChannelInputSource`] - Channel-backed source (default, used by TUI)
//! - [`StdoutEventSink`] - Simple stdout sink for CLI tools
//! - [`AutoApprovePolicy`] - Auto-approve all permissions (headless/trusted)
//! - [`DenyAllPolicy`] - Deny all permissions (sandboxed)
//! - [`InteractivePolicy`] - Always ask user (default for TUI)

mod policy;
mod sink;
mod source;

pub use policy::{
    AutoApprovePolicy, DenyAllPolicy, InteractivePolicy, PermissionPolicy, PolicyDecision,
};
pub use sink::{ChannelEventSink, EventSink, SendError, SimpleEventSink};
pub use source::{ChannelInputSource, InputSource};
