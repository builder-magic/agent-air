//! Minimal layout template

use ratatui::layout::{Constraint, Direction, Layout};

use crate::tui::widgets::widget_ids;

use super::types::{LayoutContext, LayoutResult, WidgetSizes};

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

/// Compute the minimal layout
pub fn compute(ctx: &LayoutContext, _sizes: &WidgetSizes, opts: &MinimalOptions) -> LayoutResult {
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
    fn test_minimal_layout() {
        let area = Rect::new(0, 0, 80, 24);
        let ctx = test_context(area);
        let sizes = test_sizes();

        let result = compute(&ctx, &sizes, &MinimalOptions::default());

        assert!(result.widget_areas.contains_key(widget_ids::CHAT_VIEW));
        assert!(result.widget_areas.contains_key(widget_ids::TEXT_INPUT));
        // Minimal layout doesn't include status bar as a widget
        assert!(!result.widget_areas.contains_key(widget_ids::STATUS_BAR));
    }
}
