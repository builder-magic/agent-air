//! Shared types for the layout system

use ratatui::layout::Rect;
use std::collections::{HashMap, HashSet};

use crate::themes::Theme;

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
    /// Area for the input/throbber (special handling)
    pub input_area: Option<Rect>,
}

/// Trait for custom layout providers
///
/// Implement this trait to create reusable, testable layout logic.
pub trait LayoutProvider: Send + Sync + 'static {
    /// Compute layout areas for widgets
    fn compute(&self, ctx: &LayoutContext, sizes: &WidgetSizes) -> LayoutResult;
}

/// Type alias for layout closure
pub type LayoutFn = Box<dyn Fn(Rect, &LayoutContext, &WidgetSizes) -> LayoutResult + Send + Sync>;
