//! Input Source - Provides input to the engine
//!
//! The [`InputSource`] trait defines how consumers provide input to the engine.

use std::future::Future;
use std::pin::Pin;

use tokio::sync::mpsc;

use crate::controller::ControllerInputPayload;

/// Provides input from a consumer to the agent engine.
///
/// Implementations handle receiving user messages, commands, and other
/// input from the consumer and delivering them to the engine.
///
/// # Lifecycle
///
/// The engine calls `recv()` in a loop. When the consumer closes
/// (user quits, connection dropped), `recv()` should return `None`
/// to signal shutdown.
///
/// # Example
///
/// ```ignore
/// use agent_core_runtime::agent::interface::InputSource;
/// use agent_core_runtime::controller::ControllerInputPayload;
/// use std::pin::Pin;
/// use std::future::Future;
///
/// struct MyCustomSource { /* ... */ }
///
/// impl InputSource for MyCustomSource {
///     fn recv(&mut self) -> Pin<Box<dyn Future<Output = Option<ControllerInputPayload>> + Send + '_>> {
///         Box::pin(async move {
///             // Receive from your transport
///             None
///         })
///     }
/// }
/// ```
pub trait InputSource: Send + 'static {
    /// Receive the next input from the consumer.
    ///
    /// Returns `None` when the consumer is closed and no more input
    /// will arrive. The engine will shut down when this returns `None`.
    fn recv(&mut self)
    -> Pin<Box<dyn Future<Output = Option<ControllerInputPayload>> + Send + '_>>;
}

/// Input source backed by an async channel.
///
/// This is the default source used internally. The consumer sends
/// input through a channel that this source reads from.
pub struct ChannelInputSource {
    rx: mpsc::Receiver<ControllerInputPayload>,
}

impl ChannelInputSource {
    /// Create a new channel-backed input source.
    pub fn new(rx: mpsc::Receiver<ControllerInputPayload>) -> Self {
        Self { rx }
    }

    /// Create a channel pair for input.
    ///
    /// Returns `(sender, source)` where sender is used by the consumer
    /// to send input and source is passed to the engine.
    pub fn channel(buffer: usize) -> (mpsc::Sender<ControllerInputPayload>, Self) {
        let (tx, rx) = mpsc::channel(buffer);
        (tx, Self::new(rx))
    }
}

impl InputSource for ChannelInputSource {
    fn recv(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = Option<ControllerInputPayload>> + Send + '_>> {
        Box::pin(async move { self.rx.recv().await })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::TurnId;

    #[tokio::test]
    async fn test_channel_input_source_recv() {
        let (tx, mut source) = ChannelInputSource::channel(10);

        let payload = ControllerInputPayload::data(1, "hello", TurnId::new_user_turn(1));
        tx.send(payload).await.unwrap();

        let received = source.recv().await.unwrap();
        assert_eq!(received.session_id, 1);
        assert_eq!(received.content, "hello");
    }

    #[tokio::test]
    async fn test_channel_input_source_closed() {
        let (tx, mut source) = ChannelInputSource::channel(10);

        // Drop sender to close channel
        drop(tx);

        // Should return None
        let received = source.recv().await;
        assert!(received.is_none());
    }

    #[tokio::test]
    async fn test_channel_input_source_multiple() {
        let (tx, mut source) = ChannelInputSource::channel(10);

        // Send multiple messages
        for i in 0..3 {
            let payload = ControllerInputPayload::data(
                1,
                format!("msg {}", i),
                TurnId::new_user_turn(i as i64),
            );
            tx.send(payload).await.unwrap();
        }

        // Receive all
        for i in 0..3 {
            let received = source.recv().await.unwrap();
            assert_eq!(received.content, format!("msg {}", i));
        }
    }
}
