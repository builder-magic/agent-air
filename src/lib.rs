//! Agent Core
//!
//! A TUI Framework for building terminal UI agents powered by large language models.
//!
//! This crate provides:
//!
//! ## TUI Components
//! - Permission request panels
//! - Question/answer dialogs
//! - Markdown rendering with theming
//! - Table rendering
//! - Session pickers
//! - Slash command popups
//! - Text input with cursor management
//!
//! ## Agent Infrastructure
//! - Message types for TUI-Controller communication
//! - Input routing
//! - Logging infrastructure
//! - Configuration management
//! - Base agent trait for building custom agents
//!
//! ## LLM Client
//! - Provider-agnostic LLM client interface
//! - Anthropic and OpenAI provider implementations
//! - HTTP client utilities
//!
//! ## LLM Controller
//! - Controller logic for managing LLM interactions
//! - Session management and compaction
//! - Tool execution framework
//! - Permission and user interaction registries

/// Agent infrastructure and configuration.
pub mod agent;
/// LLM client interface and provider implementations.
pub mod client;
/// LLM session controller and tool execution.
pub mod controller;
/// Terminal UI components and application framework.
pub mod tui;
