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

pub mod agent;
pub mod tui;
