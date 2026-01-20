//! LLM Agent Core
//!
//! Reusable TUI components and agent infrastructure for LLM-powered terminal applications.
//!
//! This crate provides:
//!
//! ## TUI Components (`tui` module)
//! - Permission request panels
//! - Question/answer dialogs
//! - Markdown rendering with theming
//! - Table rendering
//! - Session pickers
//! - Slash command popups
//! - Text input with cursor management
//!
//! ## Agent Infrastructure (`agent` module)
//! - Message types for TUI-Controller communication
//! - Input routing
//! - Logging infrastructure
//! - Configuration management
//! - Base agent trait for building custom agents
//!
//! ## LLM Client (`client` module)
//! - Provider-agnostic LLM client interface
//! - Anthropic and OpenAI provider implementations
//! - HTTP client utilities
//!
//! ## LLM Controller (`controller` module)
//! - Controller logic for managing LLM interactions
//! - Session management and compaction
//! - Tool execution framework
//! - Permission and user interaction registries

pub mod agent;
pub mod client;
pub mod controller;
pub mod tui;
