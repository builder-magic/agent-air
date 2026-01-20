//! TUI Widgets
//!
//! Reusable widget components for LLM-powered terminal applications.
//!
//! - [`PermissionPanel`] - Permission request panel for tool interactions
//! - [`QuestionPanel`] - Question panel for user input collection
//! - [`SessionPickerState`] - Session picker for viewing/switching sessions
//! - [`SlashPopupState`] - Slash command popup

pub mod permission_panel;
pub mod question_panel;
pub mod session_picker;
pub mod slash_popup;

pub use permission_panel::{KeyAction as PermissionKeyAction, PermissionOption, PermissionPanel};
pub use question_panel::{
    AnswerState, EnterAction, FocusItem, KeyAction as QuestionKeyAction, QuestionPanel,
};
pub use session_picker::{render_session_picker, SessionInfo, SessionPickerState};
pub use slash_popup::{
    render_slash_popup, SimpleCommand, SlashCommand as SlashCommandTrait, SlashPopupState,
};
