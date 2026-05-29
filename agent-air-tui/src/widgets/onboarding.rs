// Onboarding wizard widget for first-time setup
//
// Full-screen overlay that walks users through provider/model/API key
// configuration. Follows the ThemePicker pattern: handles its own
// side effects via a callback, returns WidgetAction::Close when done.

use std::any::Any;
use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::themes::Theme;
use crate::widgets::{Widget, WidgetAction, WidgetKeyContext, WidgetKeyResult, widget_ids};

// --- Public Types ---

/// Configuration for the onboarding wizard.
pub struct OnboardingConfig {
    /// Agent display name (shown in welcome message).
    pub agent_name: String,
    /// State directory path (shown when config is saved).
    pub state_dir: PathBuf,
    /// Available providers to choose from.
    pub providers: Vec<OnboardingProvider>,
    /// Optional custom welcome message. Falls back to a default if None.
    pub welcome_message: Option<String>,
    /// Optional exit hint shown on each step (e.g., "Ctrl-D to exit").
    pub exit_hint: Option<String>,
}

/// A provider option in the onboarding wizard.
pub struct OnboardingProvider {
    /// Provider identifier for config.yaml (e.g., "anthropic").
    pub id: String,
    /// Display name (e.g., "Anthropic").
    pub name: String,
    /// Available models for this provider.
    pub models: Vec<OnboardingModel>,
}

/// A model option in the onboarding wizard.
pub struct OnboardingModel {
    /// Model identifier for config.yaml (e.g., "claude-sonnet-4-6").
    pub id: String,
    /// Display name (e.g., "Claude Sonnet 4").
    pub name: String,
}

/// Result of a completed onboarding.
#[derive(Debug, Clone)]
pub struct OnboardingResult {
    /// Selected provider identifier.
    pub provider_id: String,
    /// Selected model identifier.
    pub model_id: String,
    /// API key entered by the user.
    pub api_key: String,
}

/// Wizard step.
enum OnboardingStep {
    Welcome,
    SelectProvider,
    SelectModel,
    EnterApiKey,
    Done,
}

/// Callback type for onboarding completion.
type OnCompleteFn = Box<dyn FnMut(&OnboardingResult) -> Result<(), String> + Send>;

// --- Helper Functions ---

/// Returns a default set of providers for the onboarding wizard.
pub fn default_providers() -> Vec<OnboardingProvider> {
    vec![
        OnboardingProvider {
            id: "anthropic".into(),
            name: "Anthropic".into(),
            models: vec![
                OnboardingModel {
                    id: "claude-opus-4-8".into(),
                    name: "Claude Opus 4.8".into(),
                },
                OnboardingModel {
                    id: "claude-sonnet-4-6".into(),
                    name: "Claude Sonnet 4.6".into(),
                },
                OnboardingModel {
                    id: "claude-haiku-4-5-20251001".into(),
                    name: "Claude Haiku 4.5".into(),
                },
            ],
        },
        OnboardingProvider {
            id: "openai".into(),
            name: "OpenAI".into(),
            models: vec![
                OnboardingModel {
                    id: "gpt-4o".into(),
                    name: "GPT-4o".into(),
                },
                OnboardingModel {
                    id: "gpt-4o-mini".into(),
                    name: "GPT-4o mini".into(),
                },
            ],
        },
        OnboardingProvider {
            id: "google".into(),
            name: "Google (Gemini)".into(),
            models: vec![
                OnboardingModel {
                    id: "gemini-2.5-pro".into(),
                    name: "Gemini 2.5 Pro".into(),
                },
                OnboardingModel {
                    id: "gemini-2.5-flash".into(),
                    name: "Gemini 2.5 Flash".into(),
                },
            ],
        },
    ]
}

/// Write a config.yaml file from onboarding results.
///
/// Creates the state directory if it does not exist.
pub fn write_config_yaml(state_dir: &Path, result: &OnboardingResult) -> Result<(), String> {
    std::fs::create_dir_all(state_dir).map_err(|e| format!("Failed to create directory: {}", e))?;

    let config_path = state_dir.join("config.yaml");
    let yaml = format!(
        "providers:\n  - provider: {}\n    api_key: {}\n    model: {}\ndefault_provider: {}\n",
        result.provider_id, result.api_key, result.model_id, result.provider_id
    );

    std::fs::write(&config_path, yaml).map_err(|e| format!("Failed to write config: {}", e))?;
    Ok(())
}

/// Mask an API key for display (show first 6 and last 4 characters).
fn mask_api_key(key: &str) -> String {
    if key.len() <= 10 {
        "*".repeat(key.len())
    } else {
        let prefix = &key[..6];
        let suffix = &key[key.len() - 4..];
        format!("{}...{}", prefix, suffix)
    }
}

/// Create a centered rectangle within the given area.
fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}

// --- Widget ---

/// Onboarding wizard widget.
///
/// A full-screen overlay that walks the user through first-time provider
/// and model configuration. Follows the ThemePicker pattern: handles its
/// own side effects via a callback, then returns `WidgetAction::Close`.
///
/// # Example
///
/// ```ignore
/// use agent_air_tui::widgets::onboarding::*;
///
/// let widget = OnboardingWidget::new(
///     OnboardingConfig {
///         agent_name: "MyAgent".into(),
///         state_dir: PathBuf::from("/home/user/.myagent"),
///         providers: default_providers(),
///         welcome_message: None,
///     },
///     |result| {
///         write_config_yaml(Path::new("/home/user/.myagent"), result)?;
///         Ok(())
///     },
/// );
/// runner.register_widget(widget);
/// ```
pub struct OnboardingWidget {
    config: OnboardingConfig,
    step: OnboardingStep,
    /// Current list selection index (reused across list steps).
    selected_index: usize,
    /// Saved provider selection from SelectProvider step.
    provider_index: usize,
    /// Saved model selection from SelectModel step.
    model_index: usize,
    /// Text buffer for API key input.
    api_key_buffer: String,
    /// Error message to display (e.g., empty API key, callback failure).
    error_message: Option<String>,
    active: bool,
    on_complete: OnCompleteFn,
    /// Stored result after successful completion (for Done step display).
    result: Option<OnboardingResult>,
}

impl OnboardingWidget {
    /// Create a new onboarding widget.
    ///
    /// The `on_complete` callback is called when the user confirms their
    /// selections. It receives the onboarding result and should persist
    /// the configuration (e.g., write config.yaml, set a DB flag).
    pub fn new(
        config: OnboardingConfig,
        on_complete: impl FnMut(&OnboardingResult) -> Result<(), String> + Send + 'static,
    ) -> Self {
        Self {
            config,
            step: OnboardingStep::Welcome,
            selected_index: 0,
            provider_index: 0,
            model_index: 0,
            api_key_buffer: String::new(),
            error_message: None,
            active: true,
            on_complete: Box::new(on_complete),
            result: None,
        }
    }

    /// Get the currently selected provider.
    fn current_provider(&self) -> Option<&OnboardingProvider> {
        self.config.providers.get(self.provider_index)
    }

    /// Get the number of items in the current list step.
    fn list_len(&self) -> usize {
        match self.step {
            OnboardingStep::SelectProvider => self.config.providers.len(),
            OnboardingStep::SelectModel => {
                self.current_provider().map(|p| p.models.len()).unwrap_or(0)
            }
            _ => 0,
        }
    }

    /// Navigate selection up.
    fn select_prev(&mut self) {
        let len = self.list_len();
        if len > 0 {
            if self.selected_index == 0 {
                self.selected_index = len - 1;
            } else {
                self.selected_index -= 1;
            }
        }
    }

    /// Navigate selection down.
    fn select_next(&mut self) {
        let len = self.list_len();
        if len > 0 {
            self.selected_index = (self.selected_index + 1) % len;
        }
    }

    /// Advance to the next step.
    fn advance(&mut self) {
        self.error_message = None;
        match self.step {
            OnboardingStep::Welcome => {
                self.step = OnboardingStep::SelectProvider;
                self.selected_index = 0;
            }
            OnboardingStep::SelectProvider => {
                self.provider_index = self.selected_index;
                self.step = OnboardingStep::SelectModel;
                self.selected_index = 0;
            }
            OnboardingStep::SelectModel => {
                self.model_index = self.selected_index;
                self.step = OnboardingStep::EnterApiKey;
                self.api_key_buffer.clear();
            }
            OnboardingStep::EnterApiKey => {
                if self.api_key_buffer.trim().is_empty() {
                    self.error_message = Some("API key cannot be empty".into());
                    return;
                }
                self.complete();
            }
            OnboardingStep::Done => {
                self.active = false;
            }
        }
    }

    /// Go back to the previous step.
    fn go_back(&mut self) {
        self.error_message = None;
        match self.step {
            OnboardingStep::Welcome | OnboardingStep::Done => {}
            OnboardingStep::SelectProvider => {
                self.step = OnboardingStep::Welcome;
            }
            OnboardingStep::SelectModel => {
                self.step = OnboardingStep::SelectProvider;
                self.selected_index = self.provider_index;
            }
            OnboardingStep::EnterApiKey => {
                self.step = OnboardingStep::SelectModel;
                self.selected_index = self.model_index;
            }
        }
    }

    /// Build the result and run the completion callback.
    fn complete(&mut self) {
        let provider = match self.config.providers.get(self.provider_index) {
            Some(p) => p,
            None => {
                self.error_message = Some("No provider selected".into());
                return;
            }
        };

        let model = match provider.models.get(self.model_index) {
            Some(m) => m,
            None => {
                self.error_message = Some("No model selected".into());
                return;
            }
        };

        let result = OnboardingResult {
            provider_id: provider.id.clone(),
            model_id: model.id.clone(),
            api_key: self.api_key_buffer.trim().to_string(),
        };

        match (self.on_complete)(&result) {
            Ok(()) => {
                self.result = Some(result);
                self.step = OnboardingStep::Done;
            }
            Err(e) => {
                self.error_message = Some(e);
            }
        }
    }

    // --- Rendering ---

    fn render_welcome(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let box_width = (area.width.saturating_sub(8)).min(120);
        let box_area = centered_rect(area, box_width, 12);

        let agent_name = &self.config.agent_name;
        let welcome = self
            .config
            .welcome_message
            .as_deref()
            .unwrap_or("Let's get you set up with an LLM provider.");

        let help_text = match &self.config.exit_hint {
            Some(hint) => format!("  Press Enter to get started  {}", hint),
            None => "  Press Enter to get started".to_string(),
        };

        let lines = vec![
            Line::from(""),
            Line::from(""),
            Line::from(Span::styled(
                format!("  Welcome to {}!", agent_name),
                theme.heading_1,
            )),
            Line::from(""),
            Line::from(format!("  {}", welcome)),
            Line::from(""),
            Line::from(""),
            Line::from(""),
            Line::from(Span::styled(help_text, theme.status_help)),
            Line::from(""),
        ];

        let block = Block::default()
            .title(format!(" {} Setup ", agent_name))
            .borders(Borders::ALL)
            .border_style(theme.popup_border);

        let paragraph = Paragraph::new(lines)
            .block(block)
            .style(theme.background.patch(theme.text));

        frame.render_widget(Clear, box_area);
        frame.render_widget(paragraph, box_area);
    }

    fn render_select_provider(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let provider_count = self.config.providers.len() as u16;
        let box_height = 8 + provider_count;
        let box_width = (area.width.saturating_sub(8)).min(120);
        let box_area = centered_rect(area, box_width, box_height);

        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled("  Select your LLM provider", theme.heading_2)),
            Line::from(""),
        ];

        let inner_width = box_area.width.saturating_sub(2) as usize;
        for (idx, provider) in self.config.providers.iter().enumerate() {
            let is_selected = idx == self.selected_index;
            let prefix = if is_selected { " > " } else { "   " };
            let text = format!("{}{}", prefix, provider.name);

            let style = if is_selected {
                theme.popup_selected_bg.patch(theme.popup_item_selected)
            } else {
                theme.popup_item
            };

            let padded = format!("{:<width$}", text, width = inner_width);
            lines.push(Line::from(Span::styled(padded, style)));
        }

        lines.push(Line::from(""));
        self.push_error_line(&mut lines, theme);
        let help_text = match &self.config.exit_hint {
            Some(hint) => format!("  Up/Down: navigate  Enter: select  Esc: back  {}", hint),
            None => "  Up/Down: navigate  Enter: select  Esc: back".to_string(),
        };
        lines.push(Line::from(Span::styled(help_text, theme.status_help)));
        lines.push(Line::from(""));

        let block = Block::default()
            .title(" Provider ")
            .borders(Borders::ALL)
            .border_style(theme.popup_border);

        let paragraph = Paragraph::new(lines)
            .block(block)
            .style(theme.background.patch(theme.text));

        frame.render_widget(Clear, box_area);
        frame.render_widget(paragraph, box_area);
    }

    fn render_select_model(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let provider = match self.current_provider() {
            Some(p) => p,
            None => return,
        };

        let model_count = provider.models.len() as u16;
        let box_height = 8 + model_count;
        let box_width = (area.width.saturating_sub(8)).min(120);
        let box_area = centered_rect(area, box_width, box_height);

        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                format!("  Select a model ({})", provider.name),
                theme.heading_2,
            )),
            Line::from(""),
        ];

        let inner_width = box_area.width.saturating_sub(2) as usize;
        for (idx, model) in provider.models.iter().enumerate() {
            let is_selected = idx == self.selected_index;
            let prefix = if is_selected { " > " } else { "   " };
            let text = format!("{}{}", prefix, model.name);

            let style = if is_selected {
                theme.popup_selected_bg.patch(theme.popup_item_selected)
            } else {
                theme.popup_item
            };

            let padded = format!("{:<width$}", text, width = inner_width);
            lines.push(Line::from(Span::styled(padded, style)));
        }

        lines.push(Line::from(""));
        self.push_error_line(&mut lines, theme);
        let help_text = match &self.config.exit_hint {
            Some(hint) => format!("  Up/Down: navigate  Enter: select  Esc: back  {}", hint),
            None => "  Up/Down: navigate  Enter: select  Esc: back".to_string(),
        };
        lines.push(Line::from(Span::styled(help_text, theme.status_help)));
        lines.push(Line::from(""));

        let block = Block::default()
            .title(" Model ")
            .borders(Borders::ALL)
            .border_style(theme.popup_border);

        let paragraph = Paragraph::new(lines)
            .block(block)
            .style(theme.background.patch(theme.text));

        frame.render_widget(Clear, box_area);
        frame.render_widget(paragraph, box_area);
    }

    fn render_enter_api_key(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let provider = match self.current_provider() {
            Some(p) => p,
            None => return,
        };

        let box_width = (area.width.saturating_sub(8)).min(120);
        let box_area = centered_rect(area, box_width, 12);

        let display_key = if self.api_key_buffer.is_empty() {
            String::new()
        } else {
            self.api_key_buffer.clone()
        };

        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                format!("  Enter your {} API key", provider.name),
                theme.heading_2,
            )),
            Line::from(""),
            Line::from(format!("  > {}_", display_key)),
            Line::from(""),
        ];

        self.push_error_line(&mut lines, theme);
        let help_text = match &self.config.exit_hint {
            Some(hint) => format!("  Enter: confirm  Esc: back  Ctrl+U: clear  {}", hint),
            None => "  Enter: confirm  Esc: back  Ctrl+U: clear".to_string(),
        };
        lines.push(Line::from(Span::styled(help_text, theme.status_help)));
        lines.push(Line::from(""));

        let block = Block::default()
            .title(" API Key ")
            .borders(Borders::ALL)
            .border_style(theme.popup_border);

        let paragraph = Paragraph::new(lines)
            .block(block)
            .style(theme.background.patch(theme.text));

        frame.render_widget(Clear, box_area);
        frame.render_widget(paragraph, box_area);
    }

    fn render_done(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let box_width = (area.width.saturating_sub(8)).min(120);
        let box_area = centered_rect(area, box_width, 16);

        let result = match &self.result {
            Some(r) => r,
            None => return,
        };

        let config_path = self.config.state_dir.join("config.yaml");

        let lines = vec![
            Line::from(""),
            Line::from(Span::styled("  Setup complete!", theme.heading_1)),
            Line::from(""),
            Line::from(format!("  Provider:  {}", result.provider_id)),
            Line::from(format!("  Model:     {}", result.model_id)),
            Line::from(format!("  API Key:   {}", mask_api_key(&result.api_key))),
            Line::from(""),
            Line::from(format!(
                "  Configuration saved to {}",
                config_path.display()
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Press Enter to start chatting.",
                theme.heading_2,
            )),
            Line::from(""),
        ];

        let block = Block::default()
            .title(" Complete ")
            .borders(Borders::ALL)
            .border_style(theme.popup_border);

        let paragraph = Paragraph::new(lines)
            .block(block)
            .style(theme.background.patch(theme.text));

        frame.render_widget(Clear, box_area);
        frame.render_widget(paragraph, box_area);
    }

    /// Append an error line if there is an error message.
    fn push_error_line<'a>(&self, lines: &mut Vec<Line<'a>>, theme: &Theme) {
        if let Some(ref err) = self.error_message {
            lines.push(Line::from(Span::styled(
                format!("  Error: {}", err),
                theme.tool_failed,
            )));
        }
    }
}

// --- Widget Trait ---

impl Widget for OnboardingWidget {
    fn id(&self) -> &'static str {
        widget_ids::ONBOARDING
    }

    fn priority(&self) -> u8 {
        250
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn handle_key(&mut self, key: KeyEvent, ctx: &WidgetKeyContext) -> WidgetKeyResult {
        if !self.active {
            return WidgetKeyResult::NotHandled;
        }

        match self.step {
            OnboardingStep::Welcome => {
                if ctx.nav.is_select(&key) {
                    self.advance();
                    return WidgetKeyResult::Handled;
                }
            }
            OnboardingStep::SelectProvider | OnboardingStep::SelectModel => {
                if ctx.nav.is_move_up(&key) {
                    self.select_prev();
                    return WidgetKeyResult::Handled;
                }
                if ctx.nav.is_move_down(&key) {
                    self.select_next();
                    return WidgetKeyResult::Handled;
                }
                if ctx.nav.is_select(&key) {
                    self.advance();
                    return WidgetKeyResult::Handled;
                }
                if ctx.nav.is_cancel(&key) {
                    self.go_back();
                    return WidgetKeyResult::Handled;
                }
            }
            OnboardingStep::EnterApiKey => {
                // Ctrl+U: clear the buffer
                if key.code == KeyCode::Char('u') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.api_key_buffer.clear();
                    self.error_message = None;
                    return WidgetKeyResult::Handled;
                }
                match key.code {
                    KeyCode::Char(c) => {
                        self.api_key_buffer.push(c);
                        self.error_message = None;
                        return WidgetKeyResult::Handled;
                    }
                    KeyCode::Backspace => {
                        self.api_key_buffer.pop();
                        self.error_message = None;
                        return WidgetKeyResult::Handled;
                    }
                    KeyCode::Enter => {
                        self.advance();
                        return WidgetKeyResult::Handled;
                    }
                    KeyCode::Esc => {
                        self.go_back();
                        return WidgetKeyResult::Handled;
                    }
                    _ => {}
                }
            }
            OnboardingStep::Done => {
                if ctx.nav.is_select(&key) || ctx.nav.is_cancel(&key) {
                    self.active = false;
                    if let Some(ref result) = self.result {
                        return WidgetKeyResult::Action(WidgetAction::CompleteOnboarding {
                            provider_id: result.provider_id.clone(),
                            model_id: result.model_id.clone(),
                            api_key: result.api_key.clone(),
                        });
                    }
                    return WidgetKeyResult::Action(WidgetAction::Close);
                }
            }
        }

        // Consume all keys when active to prevent input leak-through
        WidgetKeyResult::Handled
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if !self.active {
            return;
        }

        frame.render_widget(Clear, area);

        match self.step {
            OnboardingStep::Welcome => self.render_welcome(frame, area, theme),
            OnboardingStep::SelectProvider => self.render_select_provider(frame, area, theme),
            OnboardingStep::SelectModel => self.render_select_model(frame, area, theme),
            OnboardingStep::EnterApiKey => self.render_enter_api_key(frame, area, theme),
            OnboardingStep::Done => self.render_done(frame, area, theme),
        }
    }

    fn required_height(&self, _available: u16) -> u16 {
        0
    }

    fn blocks_input(&self) -> bool {
        self.active
    }

    fn is_overlay(&self) -> bool {
        true
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
