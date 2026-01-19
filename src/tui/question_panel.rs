//! Question panel widget for AskUserQuestions tool
//!
//! A reusable Ratatui panel that displays questions from the LLM
//! and collects structured responses from the user.
//!
//! # Navigation
//! - Up/Down/Ctrl-P/Ctrl-N: Move between focusable items
//! - Enter: Select choice or advance to next question
//! - Space: Toggle selection (for multi-choice)
//! - Esc: Cancel and close panel
//! - Tab: Jump to Submit button
//! - For text fields: all typing goes to the TextArea

use std::collections::HashSet;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use llm_controller_rs::{
    Answer, AskUserQuestionsRequest, AskUserQuestionsResponse, Question, TurnId,
};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
use tui_textarea::TextArea;

use super::theme::Theme;

/// Maximum percentage of screen height the panel can use
const MAX_PANEL_PERCENT: u16 = 70;

/// Represents a focusable item in the panel
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FocusItem {
    /// A choice option within a question
    Choice {
        question_idx: usize,
        choice_idx: usize,
    },
    /// The "Other" option for a choice question
    OtherOption { question_idx: usize },
    /// Text input for "Other" in a choice question
    OtherText { question_idx: usize },
    /// Free text input field
    TextInput { question_idx: usize },
    /// Submit button
    Submit,
    /// Cancel button
    Cancel,
}

/// Answer state for a single question
pub enum AnswerState {
    /// Single choice answer
    SingleChoice {
        selected: Option<String>,
        other_text: TextArea<'static>,
    },
    /// Multiple choice answer
    MultiChoice {
        selected: HashSet<String>,
        other_text: TextArea<'static>,
    },
    /// Free text answer
    FreeText { textarea: TextArea<'static> },
}

impl AnswerState {
    /// Create answer state from a question
    pub fn from_question(question: &Question) -> Self {
        match question {
            Question::SingleChoice { .. } => {
                let other_text = TextArea::default();
                AnswerState::SingleChoice {
                    selected: None,
                    other_text,
                }
            }
            Question::MultiChoice { .. } => {
                let other_text = TextArea::default();
                AnswerState::MultiChoice {
                    selected: HashSet::new(),
                    other_text,
                }
            }
            Question::FreeText { default_value, .. } => {
                let mut textarea = TextArea::default();
                if let Some(default) = default_value {
                    textarea.insert_str(default);
                }
                AnswerState::FreeText { textarea }
            }
        }
    }

    /// Convert to Answer for response
    pub fn to_answer(&self, question_text: &str) -> Answer {
        match self {
            AnswerState::SingleChoice {
                selected,
                other_text,
            } => {
                let other = other_text.lines().join("\n");
                // If user typed a custom answer, use that; otherwise use selected choice
                let answer_values = if !other.is_empty() {
                    vec![other]
                } else {
                    selected.iter().cloned().collect()
                };
                Answer {
                    question: question_text.to_string(),
                    answer: answer_values,
                }
            }
            AnswerState::MultiChoice {
                selected,
                other_text,
            } => {
                let other = other_text.lines().join("\n");
                let mut answer_values: Vec<String> = selected.iter().cloned().collect();
                // Add custom answer if provided
                if !other.is_empty() {
                    answer_values.push(other);
                }
                Answer {
                    question: question_text.to_string(),
                    answer: answer_values,
                }
            }
            AnswerState::FreeText { textarea } => Answer {
                question: question_text.to_string(),
                answer: vec![textarea.lines().join("\n")],
            },
        }
    }

    /// Check if a choice is selected
    pub fn is_selected(&self, choice_id: &str) -> bool {
        match self {
            AnswerState::SingleChoice { selected, .. } => {
                selected.as_ref().map(|s| s == choice_id).unwrap_or(false)
            }
            AnswerState::MultiChoice { selected, .. } => selected.contains(choice_id),
            AnswerState::FreeText { .. } => false,
        }
    }

    /// Toggle/select a choice
    pub fn select_choice(&mut self, choice_id: &str) {
        match self {
            AnswerState::SingleChoice { selected, .. } => {
                *selected = Some(choice_id.to_string());
            }
            AnswerState::MultiChoice { selected, .. } => {
                if selected.contains(choice_id) {
                    selected.remove(choice_id);
                } else {
                    selected.insert(choice_id.to_string());
                }
            }
            AnswerState::FreeText { .. } => {}
        }
    }

    /// Get the textarea for this answer (if applicable)
    pub fn textarea_mut(&mut self) -> Option<&mut TextArea<'static>> {
        match self {
            AnswerState::SingleChoice { other_text, .. }
            | AnswerState::MultiChoice { other_text, .. } => Some(other_text),
            AnswerState::FreeText { textarea } => Some(textarea),
        }
    }

    /// Check if "Other" has text
    pub fn has_other_text(&self) -> bool {
        match self {
            AnswerState::SingleChoice { other_text, .. }
            | AnswerState::MultiChoice { other_text, .. } => {
                !other_text.lines().join("").is_empty()
            }
            AnswerState::FreeText { .. } => false,
        }
    }
}

/// Result of pressing Enter
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnterAction {
    None,
    Selected,
    Submit,
    Cancel,
}

/// Result of handling a key event
#[derive(Debug, Clone)]
pub enum KeyAction {
    /// No action taken, key was handled internally
    Handled,
    /// Key was not handled (pass to parent)
    NotHandled,
    /// User submitted answers (includes tool_use_id and response)
    Submitted(String, AskUserQuestionsResponse),
    /// User cancelled
    Cancelled(String),
}

/// State for the question panel overlay
pub struct QuestionPanel {
    /// Whether the panel is active
    active: bool,
    /// Tool use ID for this interaction
    tool_use_id: String,
    /// Session ID
    session_id: i64,
    /// The questions
    request: AskUserQuestionsRequest,
    /// Turn ID
    turn_id: Option<TurnId>,
    /// Answers for each question
    answers: Vec<AnswerState>,
    /// All focusable items in order
    focus_items: Vec<FocusItem>,
    /// Current focus index
    focus_idx: usize,
}

impl QuestionPanel {
    /// Create a new inactive question panel
    pub fn new() -> Self {
        Self {
            active: false,
            tool_use_id: String::new(),
            session_id: 0,
            request: AskUserQuestionsRequest {
                questions: Vec::new(),
            },
            turn_id: None,
            answers: Vec::new(),
            focus_items: Vec::new(),
            focus_idx: 0,
        }
    }

    /// Activate the panel with questions
    pub fn activate(
        &mut self,
        tool_use_id: String,
        session_id: i64,
        request: AskUserQuestionsRequest,
        turn_id: Option<TurnId>,
    ) {
        self.active = true;
        self.tool_use_id = tool_use_id;
        self.session_id = session_id;

        // Initialize answers
        self.answers = request
            .questions
            .iter()
            .map(AnswerState::from_question)
            .collect();

        // Build focus items list
        self.focus_items = Self::build_focus_items(&request.questions);
        self.focus_idx = 0;

        self.request = request;
        self.turn_id = turn_id;
    }

    /// Build the list of focusable items from questions
    fn build_focus_items(questions: &[Question]) -> Vec<FocusItem> {
        let mut items = Vec::new();

        for (q_idx, question) in questions.iter().enumerate() {
            match question {
                Question::SingleChoice { choices, .. } | Question::MultiChoice { choices, .. } => {
                    // Each choice is focusable
                    for c_idx in 0..choices.len() {
                        items.push(FocusItem::Choice {
                            question_idx: q_idx,
                            choice_idx: c_idx,
                        });
                    }
                    // "Type Something" option - always available for custom answers
                    items.push(FocusItem::OtherOption { question_idx: q_idx });
                }
                Question::FreeText { .. } => {
                    items.push(FocusItem::TextInput { question_idx: q_idx });
                }
            }
        }

        // Add buttons
        items.push(FocusItem::Submit);
        items.push(FocusItem::Cancel);

        items
    }

    /// Deactivate the panel
    pub fn deactivate(&mut self) {
        self.active = false;
        self.tool_use_id.clear();
        self.request.questions.clear();
        self.answers.clear();
        self.focus_items.clear();
        self.focus_idx = 0;
    }

    /// Check if the panel is active
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Get the current tool use ID
    pub fn tool_use_id(&self) -> &str {
        &self.tool_use_id
    }

    /// Get the session ID
    pub fn session_id(&self) -> i64 {
        self.session_id
    }

    /// Get the current request
    pub fn request(&self) -> &AskUserQuestionsRequest {
        &self.request
    }

    /// Get the turn ID
    pub fn turn_id(&self) -> Option<&TurnId> {
        self.turn_id.as_ref()
    }

    /// Build the response from current answers
    pub fn build_response(&self) -> AskUserQuestionsResponse {
        let answers = self
            .request
            .questions
            .iter()
            .zip(self.answers.iter())
            .map(|(q, a)| a.to_answer(q.text()))
            .collect();
        AskUserQuestionsResponse { answers }
    }

    /// Get current focus item
    pub fn current_focus(&self) -> Option<&FocusItem> {
        self.focus_items.get(self.focus_idx)
    }

    /// Move focus to next item
    pub fn focus_next(&mut self) {
        if !self.focus_items.is_empty() {
            self.focus_idx = (self.focus_idx + 1) % self.focus_items.len();
        }
    }

    /// Move focus to previous item
    pub fn focus_prev(&mut self) {
        if !self.focus_items.is_empty() {
            if self.focus_idx == 0 {
                self.focus_idx = self.focus_items.len() - 1;
            } else {
                self.focus_idx -= 1;
            }
        }
    }

    /// Jump to submit button
    pub fn focus_submit(&mut self) {
        if let Some(idx) = self.focus_items.iter().position(|f| *f == FocusItem::Submit) {
            self.focus_idx = idx;
        }
    }

    /// Check if current focus is on a text input (or OtherOption which also accepts typing)
    pub fn is_text_focused(&self) -> bool {
        matches!(
            self.current_focus(),
            Some(FocusItem::TextInput { .. } | FocusItem::OtherText { .. } | FocusItem::OtherOption { .. })
        )
    }

    /// Handle Enter key - select choice or advance
    fn handle_enter(&mut self) -> EnterAction {
        match self.current_focus().cloned() {
            Some(FocusItem::Choice {
                question_idx,
                choice_idx,
            }) => {
                // Select this choice
                if let (Some(question), Some(answer)) = (
                    self.request.questions.get(question_idx),
                    self.answers.get_mut(question_idx),
                ) {
                    let choice_text = match question {
                        Question::SingleChoice { choices, .. }
                        | Question::MultiChoice { choices, .. } => {
                            choices.get(choice_idx).cloned()
                        }
                        _ => None,
                    };
                    if let Some(text) = choice_text {
                        answer.select_choice(&text);
                    }
                }
                // For single choice, advance to next question
                if matches!(
                    self.request.questions.get(question_idx),
                    Some(Question::SingleChoice { .. })
                ) {
                    self.advance_to_next_question(question_idx);
                }
                EnterAction::Selected
            }
            Some(FocusItem::OtherOption { question_idx: _ }) => {
                // Move to the Other text input
                self.focus_next();
                EnterAction::Selected
            }
            Some(FocusItem::OtherText { question_idx }) => {
                // Advance to next question
                self.advance_to_next_question(question_idx);
                EnterAction::Selected
            }
            Some(FocusItem::TextInput { question_idx }) => {
                // Advance to next question
                self.advance_to_next_question(question_idx);
                EnterAction::Selected
            }
            Some(FocusItem::Submit) => EnterAction::Submit,
            Some(FocusItem::Cancel) => EnterAction::Cancel,
            None => EnterAction::None,
        }
    }

    /// Advance focus to the first item of the next question (or Submit)
    fn advance_to_next_question(&mut self, current_question_idx: usize) {
        let next_question_idx = current_question_idx + 1;

        // Find the first focus item for the next question
        if let Some(idx) = self.focus_items.iter().position(|f| match f {
            FocusItem::Choice { question_idx, .. }
            | FocusItem::OtherOption { question_idx }
            | FocusItem::OtherText { question_idx }
            | FocusItem::TextInput { question_idx } => *question_idx == next_question_idx,
            FocusItem::Submit | FocusItem::Cancel => false,
        }) {
            self.focus_idx = idx;
        } else {
            // No more questions, go to Submit
            self.focus_submit();
        }
    }

    /// Handle key input for text areas
    fn handle_text_input(&mut self, key: KeyEvent) {
        let focus = self.current_focus().cloned();
        match focus {
            Some(FocusItem::TextInput { question_idx }) => {
                // FreeText question - forward to its textarea
                if let Some(answer) = self.answers.get_mut(question_idx) {
                    if let Some(textarea) = answer.textarea_mut() {
                        textarea.input(key);
                    }
                }
            }
            Some(FocusItem::OtherText { question_idx })
            | Some(FocusItem::OtherOption { question_idx }) => {
                // OtherOption also accepts typing - forward to the textarea
                if let Some(answer) = self.answers.get_mut(question_idx) {
                    if let Some(textarea) = answer.textarea_mut() {
                        textarea.input(key);
                    }
                    // For single choice, clear the selected option when typing in "Other"
                    if let AnswerState::SingleChoice { selected, .. } = answer {
                        *selected = None;
                    }
                }
            }
            _ => {}
        }
    }

    /// Toggle selection with Space
    fn handle_space(&mut self) {
        match self.current_focus().cloned() {
            Some(FocusItem::Choice {
                question_idx,
                choice_idx,
            }) => {
                if let (Some(question), Some(answer)) = (
                    self.request.questions.get(question_idx),
                    self.answers.get_mut(question_idx),
                ) {
                    let choice_text = match question {
                        Question::SingleChoice { choices, .. }
                        | Question::MultiChoice { choices, .. } => {
                            choices.get(choice_idx).cloned()
                        }
                        _ => None,
                    };
                    if let Some(text) = choice_text {
                        answer.select_choice(&text);
                    }
                }
            }
            Some(FocusItem::OtherOption { .. }) => {
                // Space on Other option moves to text input
                self.focus_next();
            }
            _ => {}
        }
    }

    /// Handle a key event
    ///
    /// Returns the action that should be taken based on the key press.
    pub fn handle_key(&mut self, key: KeyEvent) -> KeyAction {
        if !self.active {
            return KeyAction::NotHandled;
        }

        // Check if we're in a text input mode
        let is_text_mode = self.is_text_focused();

        match key.code {
            // Navigation (always available)
            KeyCode::Up => {
                if !is_text_mode {
                    self.focus_prev();
                    return KeyAction::Handled;
                }
            }
            KeyCode::Down => {
                if !is_text_mode {
                    self.focus_next();
                    return KeyAction::Handled;
                }
            }
            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.focus_prev();
                return KeyAction::Handled;
            }
            KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.focus_next();
                return KeyAction::Handled;
            }
            KeyCode::Char('k') if !is_text_mode => {
                self.focus_prev();
                return KeyAction::Handled;
            }
            KeyCode::Char('j') if !is_text_mode => {
                self.focus_next();
                return KeyAction::Handled;
            }

            // Tab to jump to submit
            KeyCode::Tab => {
                self.focus_submit();
                return KeyAction::Handled;
            }

            // Cancel
            KeyCode::Esc => {
                let tool_use_id = self.tool_use_id.clone();
                // Note: don't deactivate here - let the caller do it after processing
                return KeyAction::Cancelled(tool_use_id);
            }

            // Enter - select or submit
            KeyCode::Enter => {
                match self.handle_enter() {
                    EnterAction::Submit => {
                        let tool_use_id = self.tool_use_id.clone();
                        let response = self.build_response();
                        // Note: don't deactivate here - let the caller do it after processing
                        return KeyAction::Submitted(tool_use_id, response);
                    }
                    EnterAction::Cancel => {
                        let tool_use_id = self.tool_use_id.clone();
                        // Note: don't deactivate here - let the caller do it after processing
                        return KeyAction::Cancelled(tool_use_id);
                    }
                    EnterAction::Selected | EnterAction::None => {
                        return KeyAction::Handled;
                    }
                }
            }

            // Space - toggle selection (when not in text mode)
            KeyCode::Char(' ') if !is_text_mode => {
                self.handle_space();
                return KeyAction::Handled;
            }

            // Text input
            _ if is_text_mode => {
                self.handle_text_input(key);
                return KeyAction::Handled;
            }

            _ => {}
        }

        KeyAction::NotHandled
    }

    /// Calculate the panel height based on content
    pub fn panel_height(&self, max_height: u16) -> u16 {
        // Calculate height needed for content
        let mut lines = 0u16;

        for question in &self.request.questions {
            lines += 1; // Question text
            match question {
                Question::SingleChoice { choices, .. } | Question::MultiChoice { choices, .. } => {
                    lines += choices.len() as u16;
                    lines += 1; // "Type Something:" - always shown for custom answers
                }
                Question::FreeText { .. } => {
                    lines += 1; // Text input line
                }
            }
        }

        // Add: help text(1) + help blank(1) + spacing between questions + blank before buttons(1) + buttons(1) + borders(2)
        let num_questions = self.request.questions.len() as u16;
        let spacing = if num_questions > 1 { num_questions - 1 } else { 0 };
        let total = lines + spacing + 7;

        // Cap at percentage of available height, leaving room for chat and input
        let max_from_percent = (max_height * MAX_PANEL_PERCENT) / 100;
        total.min(max_from_percent).min(max_height.saturating_sub(6))
    }

    /// Render the question panel
    ///
    /// # Arguments
    /// * `frame` - The Ratatui frame to render into
    /// * `area` - The area to render the panel in
    /// * `theme` - Theme implementation for styling
    pub fn render<T: Theme>(&self, frame: &mut Frame, area: Rect, theme: &T) {
        if !self.active {
            return;
        }

        // Clear the area
        frame.render_widget(Clear, area);

        // Render panel content
        self.render_panel_content(frame, area, theme);
    }

    /// Render panel content as a single unified panel with vertical question list
    fn render_panel_content<T: Theme>(&self, frame: &mut Frame, area: Rect, theme: &T) {
        let inner_width = area.width.saturating_sub(4) as usize;
        let mut lines: Vec<Line> = Vec::new();

        // Help text at top
        let help_text = if self.is_text_focused() {
            " Type text | Enter: Next | Tab: Submit | Esc: Cancel"
        } else {
            " Up/Down: Navigate | Enter/Space: Select | Tab: Submit | Esc: Cancel"
        };
        lines.push(Line::from(Span::styled(help_text, theme.help_text())));
        lines.push(Line::from("")); // blank line after help

        // Question prefix - dots icon (with leading space for padding from border)
        const QUESTION_PREFIX: &str = " \u{2237} "; // space + proportion (double colon dots)

        // Render each question vertically
        for (q_idx, (question, answer)) in self
            .request
            .questions
            .iter()
            .zip(self.answers.iter())
            .enumerate()
        {
            // Add blank line before each question (except first)
            if q_idx > 0 {
                lines.push(Line::from(""));
            }

            // Question text with arrow prefix and required marker
            let required = if question.is_required() { "*" } else { "" };
            let q_text = format!("{}{}{}", QUESTION_PREFIX, question.text(), required);
            lines.push(Line::from(Span::styled(
                truncate_text(&q_text, inner_width),
                Style::default().add_modifier(Modifier::BOLD),
            )));

            match question {
                Question::SingleChoice { choices, .. } => {
                    self.render_choices(&mut lines, q_idx, choices, answer, false, inner_width, theme);
                }
                Question::MultiChoice { choices, .. } => {
                    self.render_choices(&mut lines, q_idx, choices, answer, true, inner_width, theme);
                }
                Question::FreeText { .. } => {
                    self.render_text_input(&mut lines, q_idx, answer, inner_width, theme);
                }
            }
        }

        // Add blank line before buttons
        lines.push(Line::from(""));

        // Add buttons - Submit and Cancel side by side
        let submit_focused = self.current_focus() == Some(&FocusItem::Submit);
        let cancel_focused = self.current_focus() == Some(&FocusItem::Cancel);

        let submit_style = if submit_focused {
            theme.button_confirm_focused()
        } else {
            theme.button_confirm()
        };
        let cancel_style = if cancel_focused {
            theme.button_cancel_focused()
        } else {
            theme.button_cancel()
        };

        let mut button_spans = vec![Span::raw("  ")];

        // Submit with indicator if focused
        if submit_focused {
            button_spans.push(Span::styled("\u{203A} ", theme.focus_indicator()));
        }
        button_spans.push(Span::styled("Submit", submit_style));
        button_spans.push(Span::raw("   "));

        // Cancel with indicator if focused
        if cancel_focused {
            button_spans.push(Span::styled("\u{203A} ", theme.focus_indicator()));
        }
        button_spans.push(Span::styled("Cancel", cancel_style));

        lines.push(Line::from(button_spans));

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.warning())
            .title(Span::styled(
                " User Input Required ",
                theme.warning().add_modifier(Modifier::BOLD),
            ));

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, area);
    }

    /// Render choice options inline
    fn render_choices<T: Theme>(
        &self,
        lines: &mut Vec<Line>,
        question_idx: usize,
        choices: &[String],
        answer: &AnswerState,
        is_multi: bool,
        inner_width: usize,
        theme: &T,
    ) {
        // Selection indicator for focused items
        const INDICATOR: &str = " \u{203A} "; // space + arrow
        const NO_INDICATOR: &str = "   "; // 3 spaces to match

        for (c_idx, choice_text) in choices.iter().enumerate() {
            let is_focused = self.current_focus()
                == Some(&FocusItem::Choice {
                    question_idx,
                    choice_idx: c_idx,
                });
            let is_selected = answer.is_selected(choice_text);

            let symbol = if is_multi {
                if is_selected { "\u{25A0}" } else { "\u{25A1}" } // filled/empty square
            } else {
                if is_selected { "\u{25CF}" } else { "\u{25CB}" } // filled/empty circle
            };

            let prefix = if is_focused { INDICATOR } else { NO_INDICATOR };
            let display_text = truncate_text(choice_text, inner_width - 8);

            if is_focused {
                lines.push(Line::from(vec![
                    Span::styled(prefix, theme.focus_indicator()),
                    Span::styled(format!("{} {}", symbol, display_text), theme.focused_text()),
                ]));
            } else {
                lines.push(Line::from(Span::styled(
                    format!("{}{} {}", prefix, symbol, display_text),
                    theme.muted_text(),
                )));
            }
        }

        // "Type Something:" option - always available for custom answers
        let other_focused = self.current_focus() == Some(&FocusItem::OtherOption { question_idx });
        let other_text_focused = self.current_focus() == Some(&FocusItem::OtherText { question_idx });
        let is_this_focused = other_focused || other_text_focused;
        let has_other = answer.has_other_text();

        // For single choice, "Type Something" is selected if:
        // - no other choice is selected AND (has text OR is currently focused)
        // For multi choice, it's selected if there's text
        let is_other_selected = match answer {
            AnswerState::SingleChoice { selected, .. } => selected.is_none() && (has_other || is_this_focused),
            AnswerState::MultiChoice { .. } => has_other,
            _ => false,
        };

        let symbol = if is_multi {
            if is_other_selected { "\u{25A0}" } else { "\u{25A1}" }
        } else {
            if is_other_selected { "\u{25CF}" } else { "\u{25CB}" }
        };

        let prefix = if is_this_focused { INDICATOR } else { NO_INDICATOR };

        // Get the text input content and cursor position
        let (other_text, cursor_col) = match answer {
            AnswerState::SingleChoice { other_text, .. }
            | AnswerState::MultiChoice { other_text, .. } => {
                let text = other_text.lines().first().cloned().unwrap_or_default();
                let col = other_text.cursor().1;
                (text, col)
            }
            _ => (String::new(), 0),
        };

        // Build the text display with cursor shown as inverse character
        if is_this_focused {
            let chars: Vec<char> = other_text.chars().collect();
            let cursor_pos = cursor_col.min(chars.len());
            let before: String = chars[..cursor_pos].iter().collect();
            let cursor_char = chars.get(cursor_pos).copied().unwrap_or(' ');
            let after: String = chars.get(cursor_pos + 1..).map(|s| s.iter().collect()).unwrap_or_default();

            lines.push(Line::from(vec![
                Span::styled(prefix, theme.focus_indicator()),
                Span::styled(format!("{} Type Something: ", symbol), theme.focused_text()),
                Span::styled(before, theme.focused_text()),
                Span::styled(cursor_char.to_string(), theme.cursor()),
                Span::styled(after, theme.focused_text()),
            ]));
        } else {
            let truncated_display = truncate_text(&other_text, inner_width - 24);
            lines.push(Line::from(Span::styled(
                format!("{}{} Type Something: {}", prefix, symbol, truncated_display),
                theme.muted_text(),
            )));
        }
    }

    /// Render free text input inline
    fn render_text_input<T: Theme>(
        &self,
        lines: &mut Vec<Line>,
        question_idx: usize,
        answer: &AnswerState,
        inner_width: usize,
        theme: &T,
    ) {
        // Selection indicator for focused items
        const INDICATOR: &str = " \u{203A} "; // space + arrow
        const NO_INDICATOR: &str = "   "; // 3 spaces to match

        let is_focused = self.current_focus() == Some(&FocusItem::TextInput { question_idx });

        let (text, cursor_col) = match answer {
            AnswerState::FreeText { textarea } => {
                let t = textarea.lines().first().cloned().unwrap_or_default();
                let col = textarea.cursor().1;
                (t, col)
            }
            _ => (String::new(), 0),
        };

        let prefix = if is_focused { INDICATOR } else { NO_INDICATOR };

        if is_focused {
            // Show cursor as inverse character
            let chars: Vec<char> = text.chars().collect();
            let cursor_pos = cursor_col.min(chars.len());
            let before: String = chars[..cursor_pos].iter().collect();
            let cursor_char = chars.get(cursor_pos).copied().unwrap_or(' ');
            let after: String = chars.get(cursor_pos + 1..).map(|s| s.iter().collect()).unwrap_or_default();

            lines.push(Line::from(vec![
                Span::styled(prefix, theme.focus_indicator()),
                Span::styled("Type Something: ", theme.focused_text()),
                Span::styled(before, theme.focused_text()),
                Span::styled(cursor_char.to_string(), theme.cursor()),
                Span::styled(after, theme.focused_text()),
            ]));
        } else {
            let display = if text.is_empty() {
                "Type Something:".to_string()
            } else {
                format!("Type Something: {}", text)
            };
            let truncated = truncate_text(&display, inner_width - 4);
            lines.push(Line::from(Span::styled(
                format!("{}{}", prefix, truncated),
                theme.muted_text(),
            )));
        }
    }
}

impl Default for QuestionPanel {
    fn default() -> Self {
        Self::new()
    }
}

/// Truncate text to fit width
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

    fn create_test_request() -> AskUserQuestionsRequest {
        AskUserQuestionsRequest {
            questions: vec![
                Question::SingleChoice {
                    text: "Choose one".to_string(),
                    choices: vec!["Option A".to_string(), "Option B".to_string()],
                    required: true,
                },
            ],
        }
    }

    #[test]
    fn test_panel_activation() {
        let mut panel = QuestionPanel::new();
        assert!(!panel.is_active());

        let request = create_test_request();
        panel.activate("tool_123".to_string(), 1, request, None);

        assert!(panel.is_active());
        assert_eq!(panel.tool_use_id(), "tool_123");
        assert_eq!(panel.session_id(), 1);

        panel.deactivate();
        assert!(!panel.is_active());
    }

    #[test]
    fn test_navigation() {
        let mut panel = QuestionPanel::new();
        let request = create_test_request();
        panel.activate("tool_1".to_string(), 1, request, None);

        // Default focus is first choice
        assert_eq!(
            panel.current_focus(),
            Some(&FocusItem::Choice {
                question_idx: 0,
                choice_idx: 0
            })
        );

        // Move to next
        panel.focus_next();
        assert_eq!(
            panel.current_focus(),
            Some(&FocusItem::Choice {
                question_idx: 0,
                choice_idx: 1
            })
        );

        // Jump to submit
        panel.focus_submit();
        assert_eq!(panel.current_focus(), Some(&FocusItem::Submit));
    }

    #[test]
    fn test_handle_key_cancel() {
        let mut panel = QuestionPanel::new();
        let request = create_test_request();
        panel.activate("tool_1".to_string(), 1, request, None);

        let action = panel.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        match action {
            KeyAction::Cancelled(tool_use_id) => {
                assert_eq!(tool_use_id, "tool_1");
            }
            _ => panic!("Expected Cancelled action"),
        }
        assert!(!panel.is_active());
    }

    #[test]
    fn test_answer_state_single_choice() {
        let mut state = AnswerState::SingleChoice {
            selected: None,
            other_text: TextArea::default(),
        };

        assert!(!state.is_selected("Option A"));
        state.select_choice("Option A");
        assert!(state.is_selected("Option A"));

        // Selecting another clears the first
        state.select_choice("Option B");
        assert!(!state.is_selected("Option A"));
        assert!(state.is_selected("Option B"));
    }

    #[test]
    fn test_answer_state_multi_choice() {
        let mut state = AnswerState::MultiChoice {
            selected: HashSet::new(),
            other_text: TextArea::default(),
        };

        state.select_choice("Option A");
        state.select_choice("Option B");
        assert!(state.is_selected("Option A"));
        assert!(state.is_selected("Option B"));

        // Toggle off
        state.select_choice("Option A");
        assert!(!state.is_selected("Option A"));
        assert!(state.is_selected("Option B"));
    }
}
