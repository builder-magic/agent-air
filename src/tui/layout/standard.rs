//! Standard vertical layout template

use ratatui::layout::{Constraint, Direction, Layout};

use crate::tui::widgets::widget_ids;

use super::types::{LayoutContext, LayoutResult, WidgetSizes};

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
    /// Widget ID for the status bar (None = no status bar)
    pub status_bar_widget_id: Option<&'static str>,
}

impl Default for StandardOptions {
    fn default() -> Self {
        Self {
            main_widget_id: widget_ids::CHAT_VIEW,
            input_widget_id: widget_ids::TEXT_INPUT,
            panel_widget_ids: vec![
                widget_ids::PERMISSION_PANEL,
                widget_ids::BATCH_PERMISSION_PANEL,
                widget_ids::QUESTION_PANEL,
            ],
            popup_widget_ids: vec![widget_ids::SLASH_POPUP],
            overlay_widget_ids: vec![
                widget_ids::THEME_PICKER,
                widget_ids::SESSION_PICKER,
            ],
            min_main_height: 5,
            fixed_input_height: None,
            status_bar_widget_id: Some(widget_ids::STATUS_BAR),
        }
    }
}

/// Compute the standard layout
pub fn compute(ctx: &LayoutContext, sizes: &WidgetSizes, opts: &StandardOptions) -> LayoutResult {
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

    // Status bar
    if let Some(status_id) = opts.status_bar_widget_id {
        if sizes.is_active(status_id) {
            constraints.push(Constraint::Length(sizes.height(status_id)));
        }
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
    if let Some(status_id) = opts.status_bar_widget_id {
        if sizes.is_active(status_id) {
            result.widget_areas.insert(status_id, chunks[chunk_idx]);
            result.render_order.push(status_id);
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::themes::Theme;
    use std::collections::{HashMap, HashSet};

    fn test_context(area: ratatui::layout::Rect) -> LayoutContext<'static> {
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
        use ratatui::layout::Rect;

        let area = Rect::new(0, 0, 80, 24);
        let ctx = test_context(area);
        let sizes = test_sizes();

        let result = compute(&ctx, &sizes, &StandardOptions::default());

        assert!(result.widget_areas.contains_key(widget_ids::CHAT_VIEW));
        assert!(result.widget_areas.contains_key(widget_ids::TEXT_INPUT));
        // Status bar is now a regular widget, but won't be in widget_areas if not active
        // (sizes.is_active returns false by default in test_sizes)
    }
}
