//! Event Sink - Receives events from the engine
//!
//! The [`EventSink`] trait defines how the engine delivers events to consumers.

use std::future::Future;
use std::pin::Pin;

use tokio::sync::mpsc;

use crate::agent::UiMessage;

/// Error when sending an event fails.
///
/// Contains the original message for retry or logging.
#[derive(Debug)]
pub struct SendError(pub UiMessage);

impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "failed to send event")
    }
}

impl std::error::Error for SendError {}

/// Receives events from the agent engine and delivers them to a consumer.
///
/// Implementations handle the transport-specific details of delivering
/// events to the user interface (TUI, WebSocket, stdout, etc.).
///
/// # Backpressure
///
/// The [`send_async`](EventSink::send_async) method supports backpressure by
/// awaiting until the consumer can accept the event. Implementations should
/// use this when the consumer has bounded capacity (e.g., channel-based).
///
/// # Thread Safety
///
/// EventSink must be `Send + Sync` to allow sharing across async tasks.
/// The engine may call `send` from multiple tasks concurrently.
///
/// # Example
///
/// ```ignore
/// use agent_air_runtime::agent::interface::{EventSink, SendError};
/// use agent_air_runtime::agent::UiMessage;
///
/// struct MyCustomSink { /* ... */ }
///
/// impl EventSink for MyCustomSink {
///     fn send(&self, event: UiMessage) -> Result<(), SendError> {
///         // Deliver event to your transport
///         Ok(())
///     }
///
///     fn clone_box(&self) -> Box<dyn EventSink> {
///         Box::new(MyCustomSink { /* ... */ })
///     }
/// }
/// ```
#[allow(clippy::result_large_err)]
pub trait EventSink: Send + Sync + 'static {
    /// Send an event to the consumer (non-blocking).
    ///
    /// Returns immediately. If the consumer cannot accept the event
    /// (e.g., buffer full), returns `Err(SendError)`.
    ///
    /// Use this for fire-and-forget scenarios or when you have your
    /// own backpressure mechanism.
    fn send(&self, event: UiMessage) -> Result<(), SendError>;

    /// Send an event to the consumer (async, with backpressure).
    ///
    /// Waits until the consumer can accept the event. This is the
    /// preferred method when backpressure is needed to avoid overwhelming
    /// slow consumers.
    ///
    /// Default implementation calls `send()` and returns immediately.
    fn send_async(
        &self,
        event: UiMessage,
    ) -> Pin<Box<dyn Future<Output = Result<(), SendError>> + Send + '_>> {
        Box::pin(async move { self.send(event) })
    }

    /// Clone this sink into a boxed trait object.
    ///
    /// Required because we need to clone sinks for internal routing
    /// but `Clone` is not object-safe.
    fn clone_box(&self) -> Box<dyn EventSink>;
}

// Allow Box<dyn EventSink> to be used as EventSink
impl EventSink for Box<dyn EventSink> {
    fn send(&self, event: UiMessage) -> Result<(), SendError> {
        (**self).send(event)
    }

    fn send_async(
        &self,
        event: UiMessage,
    ) -> Pin<Box<dyn Future<Output = Result<(), SendError>> + Send + '_>> {
        (**self).send_async(event)
    }

    fn clone_box(&self) -> Box<dyn EventSink> {
        (**self).clone_box()
    }
}

/// Event sink backed by an async channel.
///
/// This is the default sink used internally. It connects the engine
/// to a channel that the consumer reads from.
///
/// # Backpressure
///
/// When the channel is full, `send()` returns an error immediately,
/// while `send_async()` waits until space is available.
#[derive(Clone)]
pub struct ChannelEventSink {
    tx: mpsc::Sender<UiMessage>,
}

impl ChannelEventSink {
    /// Create a new channel-backed event sink.
    pub fn new(tx: mpsc::Sender<UiMessage>) -> Self {
        Self { tx }
    }
}

impl EventSink for ChannelEventSink {
    fn send(&self, event: UiMessage) -> Result<(), SendError> {
        self.tx
            .try_send(event)
            .map_err(|e| SendError(e.into_inner()))
    }

    fn send_async(
        &self,
        event: UiMessage,
    ) -> Pin<Box<dyn Future<Output = Result<(), SendError>> + Send + '_>> {
        let tx = self.tx.clone();
        Box::pin(async move { tx.send(event).await.map_err(|e| SendError(e.0)) })
    }

    fn clone_box(&self) -> Box<dyn EventSink> {
        Box::new(self.clone())
    }
}

/// Simple event sink that prints to stdout.
///
/// A minimal sink for CLI tools, debugging, or batch processing. Prints
/// LLM text output directly to stdout with basic formatting.
///
/// # Limitations
///
/// This sink is **non-interactive** and cannot handle:
/// - **Permission requests**: Use with `AutoApprovePolicy` to auto-approve
/// - **User interactions**: Use with `AutoApprovePolicy` to auto-cancel
///
/// If you need interactive permission prompts or user questions, use
/// `ChannelEventSink` with a proper frontend (TUI, WebSocket, etc.).
///
/// # Output Format
///
/// | Event | Output |
/// |-------|--------|
/// | `TextChunk` | Prints text directly (no newline) |
/// | `Complete` | Prints newline |
/// | `Error` | Prints to stderr with "Error: " prefix |
/// | `ToolExecuting` | Prints "[Tool: name]" |
/// | `ToolCompleted` | Prints errors only |
/// | `PermissionRequired` | Warning - use `AutoApprovePolicy` |
/// | `UserInteractionRequired` | Warning - use `AutoApprovePolicy` |
/// | Other events | Silently ignored |
///
/// # Example
///
/// ```ignore
/// use agent_air_runtime::agent::{
///     AgentAir, SimpleEventSink, ChannelInputSource, AutoApprovePolicy
/// };
///
/// // Simple CLI agent - must use AutoApprovePolicy
/// agent.run_with_frontend(
///     SimpleEventSink::new(),
///     input_source,
///     AutoApprovePolicy::new(),  // Required for non-interactive sink
/// )?;
/// ```
#[derive(Clone, Default)]
pub struct SimpleEventSink;

impl SimpleEventSink {
    /// Create a new simple event sink.
    pub fn new() -> Self {
        Self
    }
}

impl EventSink for SimpleEventSink {
    fn send(&self, event: UiMessage) -> Result<(), SendError> {
        use std::io::Write;

        match &event {
            UiMessage::TextChunk { text, .. } => {
                print!("{}", text);
                std::io::stdout().flush().ok();
            }
            UiMessage::Error { error, .. } => {
                eprintln!("Error: {}", error);
            }
            UiMessage::Complete { .. } => {
                println!();
            }
            UiMessage::ToolExecuting { display_name, .. } => {
                println!("[Tool: {}]", display_name);
            }
            UiMessage::ToolCompleted {
                error: Some(err), ..
            } => {
                eprintln!("[Tool error: {}]", err);
            }
            UiMessage::PermissionRequired { .. } => {
                eprintln!(
                    "Warning: SimpleEventSink received permission request. Use AutoApprovePolicy to handle permissions automatically."
                );
            }
            UiMessage::BatchPermissionRequired { .. } => {
                eprintln!(
                    "Warning: SimpleEventSink received batch permission request. Use AutoApprovePolicy to handle permissions automatically."
                );
            }
            UiMessage::UserInteractionRequired { .. } => {
                eprintln!(
                    "Warning: SimpleEventSink received user interaction request. Use AutoApprovePolicy to auto-cancel interactions."
                );
            }
            _ => {
                // Silently ignore other events
            }
        }
        Ok(())
    }

    fn clone_box(&self) -> Box<dyn EventSink> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_channel_event_sink_send() {
        let (tx, mut rx) = mpsc::channel(10);
        let sink = ChannelEventSink::new(tx);

        let event = UiMessage::System {
            session_id: 1,
            message: "test".to_string(),
        };

        sink.send(event).unwrap();

        let received = rx.recv().await.unwrap();
        match received {
            UiMessage::System {
                session_id,
                message,
            } => {
                assert_eq!(session_id, 1);
                assert_eq!(message, "test");
            }
            _ => panic!("unexpected message type"),
        }
    }

    #[tokio::test]
    async fn test_channel_event_sink_send_async() {
        let (tx, mut rx) = mpsc::channel(10);
        let sink = ChannelEventSink::new(tx);

        let event = UiMessage::System {
            session_id: 2,
            message: "async test".to_string(),
        };

        sink.send_async(event).await.unwrap();

        let received = rx.recv().await.unwrap();
        match received {
            UiMessage::System {
                session_id,
                message,
            } => {
                assert_eq!(session_id, 2);
                assert_eq!(message, "async test");
            }
            _ => panic!("unexpected message type"),
        }
    }

    #[test]
    fn test_channel_event_sink_full_channel() {
        let (tx, _rx) = mpsc::channel(1);
        let sink = ChannelEventSink::new(tx);

        // Fill the channel
        let event1 = UiMessage::System {
            session_id: 1,
            message: "first".to_string(),
        };
        sink.send(event1).unwrap();

        // Second send should fail (channel full)
        let event2 = UiMessage::System {
            session_id: 1,
            message: "second".to_string(),
        };
        let result = sink.send(event2);
        assert!(result.is_err());
    }

    #[test]
    fn test_simple_event_sink_send() {
        let sink = SimpleEventSink::new();

        // These should all succeed (simple sink never fails)
        let events = vec![
            UiMessage::TextChunk {
                session_id: 1,
                turn_id: None,
                text: "hello".to_string(),
                input_tokens: 0,
                output_tokens: 0,
            },
            UiMessage::Complete {
                session_id: 1,
                turn_id: None,
                input_tokens: 10,
                output_tokens: 20,
                stop_reason: None,
            },
            UiMessage::Error {
                session_id: 1,
                turn_id: None,
                error: "test error".to_string(),
            },
        ];

        for event in events {
            assert!(sink.send(event).is_ok());
        }
    }

    #[test]
    fn test_boxed_event_sink() {
        let (tx, _rx) = mpsc::channel(10);
        let sink: Box<dyn EventSink> = Box::new(ChannelEventSink::new(tx));

        let event = UiMessage::System {
            session_id: 1,
            message: "boxed test".to_string(),
        };

        assert!(sink.send(event).is_ok());

        // Test clone_box
        let _cloned = sink.clone_box();
    }
}
