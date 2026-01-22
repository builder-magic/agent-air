//! Split layout template

use ratatui::layout::{Constraint, Direction, Layout};

use crate::tui::widgets::widget_ids;

use super::types::{LayoutContext, LayoutResult, WidgetSizes};

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
    /// Widget ID for the status bar (None = no status bar)
    pub status_bar_widget_id: Option<&'static str>,
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
            status_bar_widget_id: Some(widget_ids::STATUS_BAR),
        }
    }
}

/// Compute the split layout
pub fn compute(ctx: &LayoutContext, sizes: &WidgetSizes, opts: &SplitOptions) -> LayoutResult {
    let mut result = LayoutResult::default();
    let area = ctx.frame_area;

    // Calculate input height
    let input_height = if ctx.show_throbber {
        3
    } else {
        (ctx.input_visual_lines as u16).max(1) + 2
    };

    let status_height = if let Some(status_id) = opts.status_bar_widget_id {
        if sizes.is_active(status_id) {
            sizes.height(status_id)
        } else {
            0
        }
    } else {
        0
    };

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

    if let Some(status_id) = opts.status_bar_widget_id {
        if sizes.is_active(status_id) && status_height > 0 {
            result.widget_areas.insert(status_id, v_chunks[2]);
            result.render_order.push(status_id);
        }
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
        .direction(opts.direction.clone())
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
