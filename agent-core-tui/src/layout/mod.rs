//! Layout system for TUI widgets
//!
//! This module provides a flexible layout system that allows agents to customize
//! how widgets are arranged in the terminal UI.
//!
//! # Layout Templates
//!
//! The easiest way to define a layout is using a template:
//!
//! ```ignore
//! // Standard layout (chat + input + status bar)
//! core.set_layout(LayoutTemplate::standard());
//!
//! // With sidebar
//! core.set_layout(LayoutTemplate::with_sidebar("file_browser", 40));
//!
//! // Minimal (no status bar)
//! core.set_layout(LayoutTemplate::minimal());
//! ```
//!
//! # Custom Layouts
//!
//! For full control, use a closure or implement `LayoutProvider`:
//!
//! ```ignore
//! core.set_layout(LayoutTemplate::custom_fn(|area, ctx, sizes| {
//!     // Use ratatui Layout directly
//!     let chunks = Layout::default()
//!         .direction(Direction::Vertical)
//!         .constraints([Constraint::Min(1), Constraint::Length(5)])
//!         .split(area);
//!
//!     LayoutResult {
//!         widget_areas: [(widget_ids::CHAT_VIEW, chunks[0])].into(),
//!         ..Default::default()
//!     }
//! }));
//! ```

mod types;
mod template;
mod standard;
mod sidebar;
mod split;
mod minimal;
pub mod helpers;

// Re-export shared types
pub use types::{LayoutContext, LayoutFn, LayoutProvider, LayoutResult, WidgetSizes};

// Re-export template
pub use template::LayoutTemplate;

// Re-export layout options
pub use standard::StandardOptions;
pub use sidebar::{SidebarOptions, SidebarPosition, SidebarWidth};
pub use split::{SplitOptions, SplitRatio};
pub use minimal::MinimalOptions;
