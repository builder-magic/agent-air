//! Agent Air Runtime
//!
//! Core runtime for building LLM-powered agents. This crate provides the
//! engine layer without any TUI dependencies.
//!
//! This crate provides:
//!
//! ## Agent Infrastructure
//! - Message types for Frontend-Controller communication
//! - Input routing
//! - Logging infrastructure
//! - Configuration management
//! - Base agent trait for building custom agents
//!
//! ## LLM Client
//! - Provider-agnostic LLM client interface
//! - Anthropic, OpenAI, Gemini, Cohere, and Bedrock provider implementations
//! - HTTP client utilities
//!
//! ## LLM Controller
//! - Controller logic for managing LLM interactions
//! - Session management and compaction
//! - Tool execution framework
//! - Permission and user interaction registries
//!
//! ## Permission System
//! - Grant-based permission model
//! - Batch permission requests
//! - Permission registry for runtime management

/// Agent infrastructure and configuration.
pub mod agent;
/// LLM client interface and provider implementations.
pub mod client;
/// LLM session controller and tool execution.
pub mod controller;
/// Permission system for controlling agent access to resources.
pub mod permissions;
/// Agent Skills support for extended capabilities.
pub mod skills;
