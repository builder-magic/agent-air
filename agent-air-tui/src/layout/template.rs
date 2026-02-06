//! Layout template enum and implementations

use ratatui::layout::{Direction, Rect};

use super::minimal::{self, MinimalOptions};
use super::sidebar::{self, SidebarOptions, SidebarWidth};
use super::split::{self, SplitOptions};
use super::standard::{self, StandardOptions};
use super::types::{LayoutContext, LayoutFn, LayoutProvider, LayoutResult, WidgetSizes};

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
    pub fn split_horizontal(left_widget_id: &'static str, right_widget_id: &'static str) -> Self {
        Self::Split(SplitOptions {
            direction: Direction::Horizontal,
            first_widget_id: left_widget_id,
            second_widget_id: right_widget_id,
            ..Default::default()
        })
    }

    /// Create a vertical split layout
    pub fn split_vertical(top_widget_id: &'static str, bottom_widget_id: &'static str) -> Self {
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
            Self::Standard(opts) => standard::compute(ctx, sizes, opts),
            Self::Sidebar(opts) => sidebar::compute(ctx, sizes, opts),
            Self::Split(opts) => split::compute(ctx, sizes, opts),
            Self::Minimal(opts) => minimal::compute(ctx, sizes, opts),
            Self::Custom(provider) => provider.compute(ctx, sizes),
            Self::CustomFn(f) => f(ctx.frame_area, ctx, sizes),
        }
    }
}

impl Default for LayoutTemplate {
    fn default() -> Self {
        Self::with_panels()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::helpers;
    use crate::themes::Theme;
    use ratatui::layout::{Constraint, Rect};
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
    fn test_custom_fn_layout() {
        let area = Rect::new(0, 0, 80, 24);
        let ctx = test_context(area);
        let sizes = test_sizes();

        let template = LayoutTemplate::custom_fn(|area, _ctx, _sizes| {
            let chunks = helpers::vstack(
                area,
                &[Constraint::Percentage(80), Constraint::Percentage(20)],
            );

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
