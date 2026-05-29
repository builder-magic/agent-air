//! Generic single-select picker panel.
//!
//! A reusable, bottom-anchored modal panel that presents a list of items and
//! lets the user pick exactly one. It is modeled on [`QuestionPanel`] in look
//! and feel, but is decoupled from any specific tool flow: the host activates
//! it with a list of items and an optional "current" marker, and receives the
//! chosen item id back via [`WidgetAction::SelectItem`].
//!
//! Typical use is to back a slash command such as `/model` (pick a model) or a
//! future `/tools` picker — the same widget serves all of them, distinguished
//! by the `picker_id` supplied at activation.
//!
//! [`QuestionPanel`]: super::question_panel::QuestionPanel
//! [`WidgetAction::SelectItem`]: super::WidgetAction::SelectItem

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::themes::Theme;

/// Default configuration values for [`SelectPanel`].
pub mod defaults {
    /// Maximum percentage of screen height the panel can use.
    pub const MAX_PANEL_PERCENT: u16 = 70;
    /// Indicator shown to the left of the focused item.
    pub const SELECTION_INDICATOR: &str = " \u{203A} ";
    /// Blank space for non-focused items (same width as the indicator).
    pub const NO_INDICATOR: &str = "   ";
    /// Marker shown next to the item that is currently in effect.
    pub const CURRENT_MARKER: &str = "* ";
    /// Blank space matching the current marker width.
    pub const NO_CURRENT_MARKER: &str = "  ";
    /// Default panel title.
    pub const TITLE: &str = " Select ";
    /// Help text shown at the top of the panel.
    pub const HELP_TEXT: &str =
        " Up/Down: Navigate \u{00B7} Enter: Select \u{00B7} Esc: Cancel \u{00B7} * = current";
}

/// A single selectable item.
#[derive(Debug, Clone)]
pub struct SelectItem {
    /// Stable identifier returned to the host on selection.
    pub id: String,
    /// Primary text shown to the user.
    pub label: String,
    /// Optional secondary text shown dimmed after the label.
    pub detail: Option<String>,
}

impl SelectItem {
    /// Create an item with just an id and label.
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            detail: None,
        }
    }

    /// Attach dimmed secondary text to the item.
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

/// Display configuration for [`SelectPanel`].
#[derive(Clone)]
pub struct SelectPanelConfig {
    /// Maximum percentage of screen height the panel can use.
    pub max_panel_percent: u16,
    /// Indicator shown to the left of the focused item.
    pub selection_indicator: String,
    /// Blank space for non-focused items.
    pub no_indicator: String,
    /// Marker shown next to the current item.
    pub current_marker: String,
    /// Blank space matching the current marker width.
    pub no_current_marker: String,
}

impl Default for SelectPanelConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl SelectPanelConfig {
    /// Create a config with default values.
    pub fn new() -> Self {
        Self {
            max_panel_percent: defaults::MAX_PANEL_PERCENT,
            selection_indicator: defaults::SELECTION_INDICATOR.to_string(),
            no_indicator: defaults::NO_INDICATOR.to_string(),
            current_marker: defaults::CURRENT_MARKER.to_string(),
            no_current_marker: defaults::NO_CURRENT_MARKER.to_string(),
        }
    }
}

/// State for the generic single-select picker.
pub struct SelectPanel {
    /// Whether the panel is active/visible.
    active: bool,
    /// Logical picker identifier (e.g. "model"), echoed back on selection.
    picker_id: String,
    /// Panel title.
    title: String,
    /// Optional prompt line shown under the help text.
    prompt: Option<String>,
    /// Items to choose from.
    items: Vec<SelectItem>,
    /// Id of the item currently in effect (marked), if any.
    current_id: Option<String>,
    /// Index of the focused item.
    focus_idx: usize,
    /// Display configuration.
    config: SelectPanelConfig,
}

impl SelectPanel {
    /// Create a new inactive panel with default configuration.
    pub fn new() -> Self {
        Self::with_config(SelectPanelConfig::new())
    }

    /// Create a new inactive panel with custom configuration.
    pub fn with_config(config: SelectPanelConfig) -> Self {
        Self {
            active: false,
            picker_id: String::new(),
            title: defaults::TITLE.to_string(),
            prompt: None,
            items: Vec::new(),
            current_id: None,
            focus_idx: 0,
            config,
        }
    }

    /// Activate the panel with a list of items.
    ///
    /// Focus starts on the current item (if `current_id` matches one), else on
    /// the first item.
    pub fn activate(
        &mut self,
        picker_id: impl Into<String>,
        title: impl Into<String>,
        prompt: Option<String>,
        items: Vec<SelectItem>,
        current_id: Option<String>,
    ) {
        self.picker_id = picker_id.into();
        self.title = title.into();
        self.prompt = prompt;
        self.focus_idx = current_id
            .as_ref()
            .and_then(|cur| items.iter().position(|it| &it.id == cur))
            .unwrap_or(0);
        self.items = items;
        self.current_id = current_id;
        self.active = true;
    }

    /// Deactivate the panel and clear transient state.
    pub fn deactivate(&mut self) {
        self.active = false;
        self.picker_id.clear();
        self.prompt = None;
        self.items.clear();
        self.current_id = None;
        self.focus_idx = 0;
    }

    /// Whether the panel is active.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// The logical picker id this panel was activated with.
    pub fn picker_id(&self) -> &str {
        &self.picker_id
    }

    /// The currently focused item, if any.
    pub fn focused_item(&self) -> Option<&SelectItem> {
        self.items.get(self.focus_idx)
    }

    /// Move focus to the previous item (wrapping).
    pub fn focus_prev(&mut self) {
        if self.items.is_empty() {
            return;
        }
        if self.focus_idx == 0 {
            self.focus_idx = self.items.len() - 1;
        } else {
            self.focus_idx -= 1;
        }
    }

    /// Move focus to the next item (wrapping).
    pub fn focus_next(&mut self) {
        if self.items.is_empty() {
            return;
        }
        self.focus_idx = (self.focus_idx + 1) % self.items.len();
    }

    /// Calculate the panel height needed for the current content.
    pub fn panel_height(&self, max_height: u16) -> u16 {
        // help(1) + blank(1) + optional prompt(1) + items + borders(2)
        let prompt_lines = if self.prompt.is_some() { 1 } else { 0 };
        let total = 2 + prompt_lines + self.items.len() as u16 + 2;

        let max_from_percent = (max_height * self.config.max_panel_percent) / 100;
        total
            .min(max_from_percent)
            .min(max_height.saturating_sub(6))
    }

    /// Render the panel into `area`.
    pub fn render_panel(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if !self.active {
            return;
        }

        frame.render_widget(Clear, area);

        let inner_width = area.width.saturating_sub(4) as usize;
        let mut lines: Vec<Line> = Vec::new();

        // Help text at the top.
        lines.push(Line::from(Span::styled(
            defaults::HELP_TEXT.to_string(),
            theme.help_text(),
        )));

        // Optional prompt line.
        if let Some(prompt) = &self.prompt {
            lines.push(Line::from(Span::styled(
                format!(" {}", prompt),
                theme.muted_text(),
            )));
        }

        lines.push(Line::from("")); // spacer before the list

        for (idx, item) in self.items.iter().enumerate() {
            let is_focused = idx == self.focus_idx;
            let is_current = self.current_id.as_deref() == Some(item.id.as_str());

            let indicator = if is_focused {
                &self.config.selection_indicator
            } else {
                &self.config.no_indicator
            };
            let marker = if is_current {
                &self.config.current_marker
            } else {
                &self.config.no_current_marker
            };

            let mut label = item.label.clone();
            if let Some(detail) = &item.detail {
                label = format!("{}  {}", label, detail);
            }
            let label = truncate_text(&label, inner_width.saturating_sub(6));

            if is_focused {
                lines.push(Line::from(vec![
                    Span::styled(indicator.clone(), theme.focus_indicator()),
                    Span::styled(marker.clone(), theme.focused_text()),
                    Span::styled(label, theme.focused_text()),
                ]));
            } else {
                lines.push(Line::from(Span::styled(
                    format!("{}{}{}", indicator, marker, label),
                    theme.muted_text(),
                )));
            }
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.warning())
            .title(Span::styled(
                self.title.clone(),
                theme.warning().add_modifier(Modifier::BOLD),
            ));

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, area);
    }
}

impl Default for SelectPanel {
    fn default() -> Self {
        Self::new()
    }
}

// --- Widget trait implementation ---

use super::{Widget, WidgetAction, WidgetKeyContext, WidgetKeyResult, widget_ids};
use std::any::Any;

impl Widget for SelectPanel {
    fn id(&self) -> &'static str {
        widget_ids::SELECT_PANEL
    }

    fn priority(&self) -> u8 {
        200 // High priority - modal panel (matches QuestionPanel)
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn handle_key(&mut self, key: KeyEvent, ctx: &WidgetKeyContext) -> WidgetKeyResult {
        if !self.active {
            return WidgetKeyResult::NotHandled;
        }

        // Navigation via configured bindings.
        if ctx.nav.is_move_up(&key) {
            self.focus_prev();
            return WidgetKeyResult::Handled;
        }
        if ctx.nav.is_move_down(&key) {
            self.focus_next();
            return WidgetKeyResult::Handled;
        }

        // Ctrl+P / Ctrl+N and j/k as additional navigation, matching the
        // other pickers in this crate.
        match key.code {
            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.focus_prev();
                return WidgetKeyResult::Handled;
            }
            KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.focus_next();
                return WidgetKeyResult::Handled;
            }
            KeyCode::Char('k') => {
                self.focus_prev();
                return WidgetKeyResult::Handled;
            }
            KeyCode::Char('j') => {
                self.focus_next();
                return WidgetKeyResult::Handled;
            }
            _ => {}
        }

        if ctx.nav.is_cancel(&key) {
            self.deactivate();
            return WidgetKeyResult::Action(WidgetAction::Close);
        }

        if ctx.nav.is_select(&key) {
            if let Some(item) = self.focused_item() {
                let action = WidgetAction::SelectItem {
                    picker_id: self.picker_id.clone(),
                    item_id: item.id.clone(),
                };
                self.deactivate();
                return WidgetKeyResult::Action(action);
            }
            // No items to select; just close.
            self.deactivate();
            return WidgetKeyResult::Action(WidgetAction::Close);
        }

        // Absorb everything else while active (modal).
        WidgetKeyResult::Handled
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        self.render_panel(frame, area, theme);
    }

    fn required_height(&self, max_height: u16) -> u16 {
        if self.active {
            self.panel_height(max_height)
        } else {
            0
        }
    }

    fn blocks_input(&self) -> bool {
        self.active
    }

    fn is_overlay(&self) -> bool {
        false
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

/// Truncate text to fit a maximum width, appending an ellipsis when cut.
fn truncate_text(text: &str, max_width: usize) -> String {
    if text.chars().count() <= max_width {
        text.to_string()
    } else {
        let truncated: String = text.chars().take(max_width.saturating_sub(3)).collect();
        format!("{}...", truncated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items() -> Vec<SelectItem> {
        vec![
            SelectItem::new("a", "Alpha"),
            SelectItem::new("b", "Beta").with_detail("recommended"),
            SelectItem::new("c", "Gamma"),
        ]
    }

    #[test]
    fn activation_focuses_current() {
        let mut panel = SelectPanel::new();
        panel.activate("model", " Model ", None, items(), Some("c".to_string()));
        assert!(panel.is_active());
        assert_eq!(panel.focused_item().map(|i| i.id.as_str()), Some("c"));
        assert_eq!(panel.picker_id(), "model");
    }

    #[test]
    fn activation_without_current_focuses_first() {
        let mut panel = SelectPanel::new();
        panel.activate("model", " Model ", None, items(), None);
        assert_eq!(panel.focused_item().map(|i| i.id.as_str()), Some("a"));
    }

    #[test]
    fn navigation_wraps() {
        let mut panel = SelectPanel::new();
        panel.activate("model", " Model ", None, items(), None);
        panel.focus_prev();
        assert_eq!(panel.focused_item().map(|i| i.id.as_str()), Some("c"));
        panel.focus_next();
        assert_eq!(panel.focused_item().map(|i| i.id.as_str()), Some("a"));
    }

    #[test]
    fn deactivate_clears_state() {
        let mut panel = SelectPanel::new();
        panel.activate("model", " Model ", None, items(), None);
        panel.deactivate();
        assert!(!panel.is_active());
        assert!(panel.focused_item().is_none());
    }
}
