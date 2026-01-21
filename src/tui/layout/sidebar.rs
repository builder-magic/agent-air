//! Sidebar layout template

use ratatui::layout::{Constraint, Direction, Layout};

use super::standard::{self, StandardOptions};
use super::types::{LayoutContext, LayoutResult, WidgetSizes};

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

/// Compute the sidebar layout
pub fn compute(ctx: &LayoutContext, sizes: &WidgetSizes, opts: &SidebarOptions) -> LayoutResult {
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
    let mut result = standard::compute(&main_ctx, sizes, &opts.main_options);

    // Add sidebar
    result.widget_areas.insert(opts.sidebar_widget_id, sidebar_area);
    // Insert sidebar at beginning of render order (renders first, behind main)
    result.render_order.insert(0, opts.sidebar_widget_id);

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::themes::Theme;
    use crate::tui::widgets::widget_ids;
    use ratatui::layout::Rect;
    use std::collections::{HashMap, HashSet};

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
    fn test_sidebar_layout() {
        let area = Rect::new(0, 0, 100, 24);
        let ctx = test_context(area);
        let sizes = test_sizes();

        let opts = SidebarOptions {
            sidebar_widget_id: "file_browser",
            sidebar_width: SidebarWidth::Fixed(30),
            ..Default::default()
        };

        let result = compute(&ctx, &sizes, &opts);

        assert!(result.widget_areas.contains_key(widget_ids::CHAT_VIEW));
        assert!(result.widget_areas.contains_key("file_browser"));

        let sidebar_area = result.widget_areas.get("file_browser").unwrap();
        assert_eq!(sidebar_area.width, 30);
    }
}
