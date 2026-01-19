// Message types for conversation tracking

use super::{ContentBlock, MessageRole, TurnId};

/// Error information for failed operations
#[derive(Debug, Clone, PartialEq)]
pub struct ErrorInfo {
    /// Error code
    pub code: String,
    /// Error message
    pub message: String,
}

/// A message in the conversation
#[derive(Debug, Clone)]
pub enum Message {
    /// A user message
    User(UserMessage),
    /// An assistant message
    Assistant(AssistantMessage),
}

impl Message {
    /// Returns the message ID
    pub fn id(&self) -> &str {
        match self {
            Self::User(m) => &m.id,
            Self::Assistant(m) => &m.id,
        }
    }

    /// Returns the session ID
    pub fn session_id(&self) -> &str {
        match self {
            Self::User(m) => &m.session_id,
            Self::Assistant(m) => &m.session_id,
        }
    }

    /// Returns the turn ID
    pub fn turn_id(&self) -> &TurnId {
        match self {
            Self::User(m) => &m.turn_id,
            Self::Assistant(m) => &m.turn_id,
        }
    }

    /// Returns the role of this message
    pub fn role(&self) -> MessageRole {
        match self {
            Self::User(_) => MessageRole::User,
            Self::Assistant(_) => MessageRole::Assistant,
        }
    }

    /// Returns the creation timestamp
    pub fn created_at(&self) -> i64 {
        match self {
            Self::User(m) => m.created_at,
            Self::Assistant(m) => m.created_at,
        }
    }

    /// Returns the content blocks
    pub fn content(&self) -> &[ContentBlock] {
        match self {
            Self::User(m) => &m.content,
            Self::Assistant(m) => &m.content,
        }
    }

    /// Returns mutable content blocks for modification (e.g., compaction)
    pub fn content_mut(&mut self) -> &mut Vec<ContentBlock> {
        match self {
            Self::User(m) => &mut m.content,
            Self::Assistant(m) => &mut m.content,
        }
    }

    /// Returns true if this is a user message
    pub fn is_user(&self) -> bool {
        matches!(self, Self::User(_))
    }

    /// Returns true if this is an assistant message
    pub fn is_assistant(&self) -> bool {
        matches!(self, Self::Assistant(_))
    }
}

/// A message from the user
#[derive(Debug, Clone)]
pub struct UserMessage {
    /// Unique message ID
    pub id: String,
    /// Session this message belongs to
    pub session_id: String,
    /// Turn ID for conversation tracking
    pub turn_id: TurnId,
    /// When the message was created (unix timestamp ms)
    pub created_at: i64,
    /// Content blocks in this message
    pub content: Vec<ContentBlock>,
}

/// A message from the assistant
#[derive(Debug, Clone)]
pub struct AssistantMessage {
    /// Unique message ID
    pub id: String,
    /// Session this message belongs to
    pub session_id: String,
    /// Turn ID for conversation tracking
    pub turn_id: TurnId,
    /// Parent message ID
    pub parent_id: String,
    /// When the message was created (unix timestamp ms)
    pub created_at: i64,
    /// When the message was completed (unix timestamp ms)
    pub completed_at: Option<i64>,
    /// Model identifier
    pub model_id: String,
    /// Provider identifier
    pub provider_id: String,
    /// Input token count
    pub input_tokens: i64,
    /// Output token count
    pub output_tokens: i64,
    /// Cache read token count
    pub cache_read_tokens: i64,
    /// Cache write token count
    pub cache_write_tokens: i64,
    /// Reason the response finished
    pub finish_reason: Option<String>,
    /// Error if the response failed
    pub error: Option<ErrorInfo>,
    /// Content blocks in this message
    pub content: Vec<ContentBlock>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_message() {
        let msg = Message::User(UserMessage {
            id: "msg_1".to_string(),
            session_id: "sess_1".to_string(),
            turn_id: TurnId::new_user_turn(1),
            created_at: 1234567890,
            content: vec![ContentBlock::text("Hello")],
        });

        assert!(msg.is_user());
        assert!(!msg.is_assistant());
        assert_eq!(msg.role(), MessageRole::User);
        assert_eq!(msg.id(), "msg_1");
        assert_eq!(msg.turn_id().to_string(), "u1");
    }

    #[test]
    fn test_assistant_message() {
        let msg = Message::Assistant(AssistantMessage {
            id: "msg_2".to_string(),
            session_id: "sess_1".to_string(),
            turn_id: TurnId::new_assistant_turn(1),
            parent_id: "msg_1".to_string(),
            created_at: 1234567891,
            completed_at: Some(1234567892),
            model_id: "claude-3".to_string(),
            provider_id: "anthropic".to_string(),
            input_tokens: 100,
            output_tokens: 50,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            finish_reason: Some("end_turn".to_string()),
            error: None,
            content: vec![ContentBlock::text("Hi there!")],
        });

        assert!(!msg.is_user());
        assert!(msg.is_assistant());
        assert_eq!(msg.role(), MessageRole::Assistant);
        assert_eq!(msg.id(), "msg_2");
        assert_eq!(msg.turn_id().to_string(), "a1");
    }
}
