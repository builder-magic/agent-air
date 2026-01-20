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

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use std::collections::{HashMap, HashSet};

use super::themes::Theme;
use super::widgets::widget_ids;

/// Context available during layout computation
pub struct LayoutContext<'a> {
    /// Total frame area
    pub frame_area: Rect,
    /// Whether the throbber/spinner is showing
    pub show_throbber: bool,
    /// Number of visual lines in the input widget
    pub input_visual_lines: usize,
    /// Current theme
    pub theme: &'a Theme,
    /// Set of currently active widget IDs
    pub active_widgets: HashSet<&'static str>,
}

/// Pre-computed widget size information
pub struct WidgetSizes {
    /// Required heights for each widget (from Widget::required_height)
    pub heights: HashMap<&'static str, u16>,
    /// Whether each widget is currently active
    pub is_active: HashMap<&'static str, bool>,
}

impl WidgetSizes {
    /// Get the required height for a widget
    pub fn height(&self, id: &str) -> u16 {
        self.heights.get(id).copied().unwrap_or(0)
    }

    /// Check if a widget is active
    pub fn is_active(&self, id: &str) -> bool {
        self.is_active.get(id).copied().unwrap_or(false)
    }
}

/// Result of layout computation
#[derive(Default)]
pub struct LayoutResult {
    /// Area assigned to each widget (by widget ID)
    pub widget_areas: HashMap<&'static str, Rect>,
    /// Order to render widgets (first = bottom layer)
    pub render_order: Vec<&'static str>,
    /// Area for the status bar (special, not a widget)
    pub status_bar_area: Option<Rect>,
    /// Area for the input/throbber (special handling)
    pub input_area: Option<Rect>,
}

/// Trait for custom layout providers
///
/// Implement this trait to create reusable, testable layout logic.
pub trait LayoutProvider: Send + Sync + 'static {
    /// Compute layout areas for widgets
    fn compute(
        &self,
        ctx: &LayoutContext,
        sizes: &WidgetSizes,
    ) -> LayoutResult;
}

/// Type alias for layout closure
pub type LayoutFn = Box<dyn Fn(Rect, &LayoutContext, &WidgetSizes) -> LayoutResult + Send + Sync>;

/// Layout templates with customization options
///
/// Templates provide common layout patterns. Use `Custom` or `CustomFn`
/// for full control with ratatui.
pub enum LayoutTemplate {
    /// Standard vertical layout: chat (fills), panels, input, status bar
    Standard(StandardOptions),

    /// Sidebar layout: main content + sidebar
    Sidebar(SidebarOptions),

    /// Split layout: two main areas side by side or stacked
    Split(SplitOptions),

    /// Minimal layout: just chat and input, no status bar
    Minimal(MinimalOptions),

    /// Custom layout using a LayoutProvider implementation
    Custom(Box<dyn LayoutProvider>),

    /// Custom layout using a closure
    CustomFn(LayoutFn),
}

// ============ Standard Layout Options ============

/// Options for the standard vertical layout
#[derive(Clone)]
pub struct StandardOptions {
    /// Widget ID for the main content area (default: CHAT_VIEW)
    pub main_widget_id: &'static str,
    /// Widget ID for the input area (default: TEXT_INPUT)
    pub input_widget_id: &'static str,
    /// Widget IDs for panel widgets (shown between main and input when active)
    pub panel_widget_ids: Vec<&'static str>,
    /// Widget IDs for popup widgets (shown above input when active)
    pub popup_widget_ids: Vec<&'static str>,
    /// Widget IDs for overlay widgets (rendered on top of everything)
    pub overlay_widget_ids: Vec<&'static str>,
    /// Minimum height for the main content area
    pub min_main_height: u16,
    /// Fixed input height (None = auto-size from content)
    pub fixed_input_height: Option<u16>,
    /// Whether to show the status bar
    pub show_status_bar: bool,
    /// Height of the status bar
    pub status_bar_height: u16,
}

impl Default for StandardOptions {
    fn default() -> Self {
        Self {
            main_widget_id: widget_ids::CHAT_VIEW,
            input_widget_id: widget_ids::TEXT_INPUT,
            panel_widget_ids: vec![
                widget_ids::PERMISSION_PANEL,
                widget_ids::QUESTION_PANEL,
            ],
            popup_widget_ids: vec![widget_ids::SLASH_POPUP],
            overlay_widget_ids: vec![
                widget_ids::THEME_PICKER,
                widget_ids::SESSION_PICKER,
            ],
            min_main_height: 5,
            fixed_input_height: None,
            show_status_bar: true,
            status_bar_height: 2,
        }
    }
}

// ============ Sidebar Layout Options ============

/// Options for sidebar layout
#[derive(Clone)]
pub struct SidebarOptions {
    /// Options for the main content area
    pub main_options: StandardOptions,
    /// Widget ID for the sidebar
    pub sidebar_widget_id: &'static str,
    /// Width of the sidebar
    pub sidebar_width: SidebarWidth,
    /// Position of the sidebar
    pub sidebar_position: SidebarPosition,
}

/// Sidebar width specification
#[derive(Clone)]
pub enum SidebarWidth {
    /// Fixed width in columns
    Fixed(u16),
    /// Percentage of total width
    Percentage(u16),
    /// Minimum width (sidebar gets this, main gets rest)
    Min(u16),
}

impl From<u16> for SidebarWidth {
    fn from(width: u16) -> Self {
        Self::Fixed(width)
    }
}

/// Sidebar position
#[derive(Clone, Copy, Default)]
pub enum SidebarPosition {
    Left,
    #[default]
    Right,
}

impl Default for SidebarOptions {
    fn default() -> Self {
        Self {
            main_options: StandardOptions::default(),
            sidebar_widget_id: "sidebar",
            sidebar_width: SidebarWidth::Fixed(30),
            sidebar_position: SidebarPosition::Right,
        }
    }
}

// ============ Split Layout Options ============

/// Options for split layout (two main areas)
#[derive(Clone)]
pub struct SplitOptions {
    /// Direction of the split
    pub direction: Direction,
    /// Widget ID for the first (left/top) area
    pub first_widget_id: &'static str,
    /// Widget ID for the second (right/bottom) area
    pub second_widget_id: &'static str,
    /// How to split the space
    pub split: SplitRatio,
    /// Widget ID for input (shared below both areas)
    pub input_widget_id: &'static str,
    /// Whether to show status bar
    pub show_status_bar: bool,
}

/// Split ratio specification
#[derive(Clone)]
pub enum SplitRatio {
    /// Equal split (50/50)
    Equal,
    /// Percentage for first area (remainder goes to second)
    Percentage(u16),
    /// Fixed size for first area
    FirstFixed(u16),
    /// Fixed size for second area
    SecondFixed(u16),
}

impl Default for SplitOptions {
    fn default() -> Self {
        Self {
            direction: Direction::Horizontal,
            first_widget_id: widget_ids::CHAT_VIEW,
            second_widget_id: "secondary",
            split: SplitRatio::Equal,
            input_widget_id: widget_ids::TEXT_INPUT,
            show_status_bar: true,
        }
    }
}

// ============ Minimal Layout Options ============

/// Options for minimal layout (no status bar, no panels)
#[derive(Clone)]
pub struct MinimalOptions {
    /// Widget ID for the main content area
    pub main_widget_id: &'static str,
    /// Widget ID for the input area
    pub input_widget_id: &'static str,
    /// Fixed input height (None = auto-size)
    pub fixed_input_height: Option<u16>,
}

impl Default for MinimalOptions {
    fn default() -> Self {
        Self {
            main_widget_id: widget_ids::CHAT_VIEW,
            input_widget_id: widget_ids::TEXT_INPUT,
            fixed_input_height: None,
        }
    }
}

// ============ LayoutTemplate Implementation ============

impl LayoutTemplate {
    // --- Constructors ---

    /// Create a standard layout with default options
    pub fn standard() -> Self {
        Self::Standard(StandardOptions::default())
    }

    /// Create a standard layout with panels (permission, question, slash popup)
    pub fn with_panels() -> Self {
        Self::Standard(StandardOptions::default())
    }

    /// Create a sidebar layout
    pub fn with_sidebar(sidebar_widget_id: &'static str, width: impl Into<SidebarWidth>) -> Self {
        Self::Sidebar(SidebarOptions {
            sidebar_widget_id,
            sidebar_width: width.into(),
            ..Default::default()
        })
    }

    /// Create a minimal layout (no status bar, no panels)
    pub fn minimal() -> Self {
        Self::Minimal(MinimalOptions::default())
    }

    /// Create a horizontal split layout
    pub fn split_horizontal(
        left_widget_id: &'static str,
        right_widget_id: &'static str,
    ) -> Self {
        Self::Split(SplitOptions {
            direction: Direction::Horizontal,
            first_widget_id: left_widget_id,
            second_widget_id: right_widget_id,
            ..Default::default()
        })
    }

    /// Create a vertical split layout
    pub fn split_vertical(
        top_widget_id: &'static str,
        bottom_widget_id: &'static str,
    ) -> Self {
        Self::Split(SplitOptions {
            direction: Direction::Vertical,
            first_widget_id: top_widget_id,
            second_widget_id: bottom_widget_id,
            ..Default::default()
        })
    }

    /// Create a custom layout using a LayoutProvider
    pub fn custom<P: LayoutProvider>(provider: P) -> Self {
        Self::Custom(Box::new(provider))
    }

    /// Create a custom layout using a closure
    pub fn custom_fn<F>(f: F) -> Self
    where
        F: Fn(Rect, &LayoutContext, &WidgetSizes) -> LayoutResult + Send + Sync + 'static,
    {
        Self::CustomFn(Box::new(f))
    }

    // --- Compute Layout ---

    /// Compute the layout for the given context
    pub fn compute(&self, ctx: &LayoutContext, sizes: &WidgetSizes) -> LayoutResult {
        match self {
            Self::Standard(opts) => Self::compute_standard(ctx, sizes, opts),
            Self::Sidebar(opts) => Self::compute_sidebar(ctx, sizes, opts),
            Self::Split(opts) => Self::compute_split(ctx, sizes, opts),
            Self::Minimal(opts) => Self::compute_minimal(ctx, sizes, opts),
            Self::Custom(provider) => provider.compute(ctx, sizes),
            Self::CustomFn(f) => f(ctx.frame_area, ctx, sizes),
        }
    }

    fn compute_standard(
        ctx: &LayoutContext,
        sizes: &WidgetSizes,
        opts: &StandardOptions,
    ) -> LayoutResult {
        let mut result = LayoutResult::default();
        let area = ctx.frame_area;

        // Calculate heights for dynamic elements
        let input_height = opts.fixed_input_height.unwrap_or_else(|| {
            if ctx.show_throbber {
                3
            } else {
                (ctx.input_visual_lines as u16).max(1) + 2
            }
        });

        // Calculate panel height (active panels only)
        let panel_height: u16 = opts
            .panel_widget_ids
            .iter()
            .filter(|id| sizes.is_active(id))
            .map(|id| sizes.height(id))
            .sum();

        // Calculate popup height (active popups only)
        let popup_height: u16 = opts
            .popup_widget_ids
            .iter()
            .filter(|id| sizes.is_active(id))
            .map(|id| sizes.height(id))
            .sum();

        // Build constraints
        let mut constraints = vec![Constraint::Min(opts.min_main_height)]; // Main

        if panel_height > 0 {
            constraints.push(Constraint::Length(panel_height));
        }

        if popup_height > 0 {
            constraints.push(Constraint::Length(popup_height));
        }

        constraints.push(Constraint::Length(input_height)); // Input

        if opts.show_status_bar {
            constraints.push(Constraint::Length(opts.status_bar_height));
        }

        // Apply layout
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(area);

        // Map chunks to widgets
        let mut chunk_idx = 0;

        // Main content
        result.widget_areas.insert(opts.main_widget_id, chunks[chunk_idx]);
        result.render_order.push(opts.main_widget_id);
        chunk_idx += 1;

        // Panels (split evenly if multiple active)
        if panel_height > 0 {
            let active_panels: Vec<_> = opts
                .panel_widget_ids
                .iter()
                .filter(|id| sizes.is_active(id))
                .collect();

            if active_panels.len() == 1 {
                result.widget_areas.insert(active_panels[0], chunks[chunk_idx]);
                result.render_order.push(active_panels[0]);
            } else if !active_panels.is_empty() {
                // Split panel area among active panels
                let panel_constraints: Vec<_> = active_panels
                    .iter()
                    .map(|id| Constraint::Length(sizes.height(id)))
                    .collect();
                let panel_chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints(panel_constraints)
                    .split(chunks[chunk_idx]);

                for (i, id) in active_panels.iter().enumerate() {
                    result.widget_areas.insert(id, panel_chunks[i]);
                    result.render_order.push(id);
                }
            }
            chunk_idx += 1;
        }

        // Popups
        if popup_height > 0 {
            let active_popups: Vec<_> = opts
                .popup_widget_ids
                .iter()
                .filter(|id| sizes.is_active(id))
                .collect();

            for id in active_popups {
                result.widget_areas.insert(id, chunks[chunk_idx]);
                result.render_order.push(id);
            }
            chunk_idx += 1;
        }

        // Input
        result.widget_areas.insert(opts.input_widget_id, chunks[chunk_idx]);
        result.input_area = Some(chunks[chunk_idx]);
        result.render_order.push(opts.input_widget_id);
        chunk_idx += 1;

        // Status bar
        if opts.show_status_bar {
            result.status_bar_area = Some(chunks[chunk_idx]);
        }

        // Overlays (use full frame area, added last to render on top)
        for id in &opts.overlay_widget_ids {
            if sizes.is_active(id) {
                result.widget_areas.insert(id, area);
                result.render_order.push(id);
            }
        }

        result
    }

    fn compute_sidebar(
        ctx: &LayoutContext,
        sizes: &WidgetSizes,
        opts: &SidebarOptions,
    ) -> LayoutResult {
        let area = ctx.frame_area;

        // Compute sidebar width constraint
        let sidebar_constraint = match opts.sidebar_width {
            SidebarWidth::Fixed(w) => Constraint::Length(w),
            SidebarWidth::Percentage(p) => Constraint::Percentage(p),
            SidebarWidth::Min(w) => Constraint::Min(w),
        };

        // Split horizontally
        let h_constraints = match opts.sidebar_position {
            SidebarPosition::Left => vec![sidebar_constraint, Constraint::Min(1)],
            SidebarPosition::Right => vec![Constraint::Min(1), sidebar_constraint],
        };

        let h_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(h_constraints)
            .split(area);

        let (main_area, sidebar_area) = match opts.sidebar_position {
            SidebarPosition::Left => (h_chunks[1], h_chunks[0]),
            SidebarPosition::Right => (h_chunks[0], h_chunks[1]),
        };

        // Compute main area layout using standard layout
        let main_ctx = LayoutContext {
            frame_area: main_area,
            show_throbber: ctx.show_throbber,
            input_visual_lines: ctx.input_visual_lines,
            theme: ctx.theme,
            active_widgets: ctx.active_widgets.clone(),
        };
        let mut result = Self::compute_standard(&main_ctx, sizes, &opts.main_options);

        // Add sidebar
        result.widget_areas.insert(opts.sidebar_widget_id, sidebar_area);
        // Insert sidebar at beginning of render order (renders first, behind main)
        result.render_order.insert(0, opts.sidebar_widget_id);

        result
    }

    fn compute_split(
        ctx: &LayoutContext,
        _sizes: &WidgetSizes,
        opts: &SplitOptions,
    ) -> LayoutResult {
        let mut result = LayoutResult::default();
        let area = ctx.frame_area;

        // Calculate input height
        let input_height = if ctx.show_throbber {
            3
        } else {
            (ctx.input_visual_lines as u16).max(1) + 2
        };

        let status_height = if opts.show_status_bar { 2 } else { 0 };

        // First split: main content vs input/status
        let v_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(5),
                Constraint::Length(input_height),
                Constraint::Length(status_height),
            ])
            .split(area);

        let content_area = v_chunks[0];
        result.input_area = Some(v_chunks[1]);
        result.widget_areas.insert(opts.input_widget_id, v_chunks[1]);

        if opts.show_status_bar {
            result.status_bar_area = Some(v_chunks[2]);
        }

        // Split content area
        let split_constraint = match opts.split {
            SplitRatio::Equal => Constraint::Percentage(50),
            SplitRatio::Percentage(p) => Constraint::Percentage(p),
            SplitRatio::FirstFixed(w) => Constraint::Length(w),
            SplitRatio::SecondFixed(_) => Constraint::Min(1), // Second gets fixed below
        };

        let second_constraint = match opts.split {
            SplitRatio::SecondFixed(w) => Constraint::Length(w),
            _ => Constraint::Min(1),
        };

        let content_chunks = Layout::default()
            .direction(opts.direction)
            .constraints([split_constraint, second_constraint])
            .split(content_area);

        result.widget_areas.insert(opts.first_widget_id, content_chunks[0]);
        result.widget_areas.insert(opts.second_widget_id, content_chunks[1]);

        result.render_order = vec![
            opts.first_widget_id,
            opts.second_widget_id,
            opts.input_widget_id,
        ];

        result
    }

    fn compute_minimal(
        ctx: &LayoutContext,
        _sizes: &WidgetSizes,
        opts: &MinimalOptions,
    ) -> LayoutResult {
        let mut result = LayoutResult::default();
        let area = ctx.frame_area;

        let input_height = opts.fixed_input_height.unwrap_or_else(|| {
            if ctx.show_throbber {
                3
            } else {
                (ctx.input_visual_lines as u16).max(1) + 2
            }
        });

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(input_height)])
            .split(area);

        result.widget_areas.insert(opts.main_widget_id, chunks[0]);
        result.widget_areas.insert(opts.input_widget_id, chunks[1]);
        result.input_area = Some(chunks[1]);

        result.render_order = vec![opts.main_widget_id, opts.input_widget_id];

        result
    }
}

impl Default for LayoutTemplate {
    fn default() -> Self {
        Self::with_panels()
    }
}

// ============ Layout Helper Functions ============

/// Helper functions for building custom layouts with ratatui
pub mod helpers {
    use super::*;

    /// Create a vertical stack layout
    pub fn vstack(area: Rect, constraints: &[Constraint]) -> Vec<Rect> {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(area)
            .to_vec()
    }

    /// Create a horizontal stack layout
    pub fn hstack(area: Rect, constraints: &[Constraint]) -> Vec<Rect> {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints(constraints)
            .split(area)
            .to_vec()
    }

    /// Create a centered area within a parent area
    pub fn centered(area: Rect, width: u16, height: u16) -> Rect {
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        Rect::new(x, y, width.min(area.width), height.min(area.height))
    }

    /// Create margin around an area
    pub fn with_margin(area: Rect, margin: u16) -> Rect {
        Rect::new(
            area.x + margin,
            area.y + margin,
            area.width.saturating_sub(margin * 2),
            area.height.saturating_sub(margin * 2),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_context(area: Rect) -> LayoutContext<'static> {
        static THEME: std::sync::LazyLock<Theme> = std::sync::LazyLock::new(Theme::default);
        LayoutContext {
            frame_area: area,
            show_throbber: false,
            input_visual_lines: 1,
            theme: &THEME,
            active_widgets: HashSet::new(),
        }
    }

    fn test_sizes() -> WidgetSizes {
        WidgetSizes {
            heights: HashMap::new(),
            is_active: HashMap::new(),
        }
    }

    #[test]
    fn test_standard_layout() {
        let area = Rect::new(0, 0, 80, 24);
        let ctx = test_context(area);
        let sizes = test_sizes();

        let result = LayoutTemplate::standard().compute(&ctx, &sizes);

        assert!(result.widget_areas.contains_key(widget_ids::CHAT_VIEW));
        assert!(result.widget_areas.contains_key(widget_ids::TEXT_INPUT));
        assert!(result.status_bar_area.is_some());
    }

    #[test]
    fn test_minimal_layout() {
        let area = Rect::new(0, 0, 80, 24);
        let ctx = test_context(area);
        let sizes = test_sizes();

        let result = LayoutTemplate::minimal().compute(&ctx, &sizes);

        assert!(result.widget_areas.contains_key(widget_ids::CHAT_VIEW));
        assert!(result.widget_areas.contains_key(widget_ids::TEXT_INPUT));
        assert!(result.status_bar_area.is_none());
    }

    #[test]
    fn test_sidebar_layout() {
        let area = Rect::new(0, 0, 100, 24);
        let ctx = test_context(area);
        let sizes = test_sizes();

        let result = LayoutTemplate::with_sidebar("file_browser", 30).compute(&ctx, &sizes);

        assert!(result.widget_areas.contains_key(widget_ids::CHAT_VIEW));
        assert!(result.widget_areas.contains_key("file_browser"));

        let sidebar_area = result.widget_areas.get("file_browser").unwrap();
        assert_eq!(sidebar_area.width, 30);
    }

    #[test]
    fn test_custom_fn_layout() {
        let area = Rect::new(0, 0, 80, 24);
        let ctx = test_context(area);
        let sizes = test_sizes();

        let template = LayoutTemplate::custom_fn(|area, _ctx, _sizes| {
            let chunks = helpers::vstack(area, &[
                Constraint::Percentage(80),
                Constraint::Percentage(20),
            ]);

            let mut result = LayoutResult::default();
            result.widget_areas.insert("custom_main", chunks[0]);
            result.widget_areas.insert("custom_footer", chunks[1]);
            result.render_order = vec!["custom_main", "custom_footer"];
            result
        });

        let result = template.compute(&ctx, &sizes);

        assert!(result.widget_areas.contains_key("custom_main"));
        assert!(result.widget_areas.contains_key("custom_footer"));
    }
}
