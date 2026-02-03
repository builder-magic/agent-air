// Message router for TUI-Controller communication
//
// InputRouter reads user input from the TUI and forwards it to the controller.
// The controller-to-TUI direction is handled by the event callback in the agent.

use std::sync::Arc;

use crate::controller::LLMController;
use tokio_util::sync::CancellationToken;

use super::messages::channels::ToControllerRx;

/// Routes messages from the TUI to the controller.
///
/// This component runs as an async task and reads from the `from_tui` channel,
/// forwarding each message to the controller via `send_input()`.
pub struct InputRouter {
    controller: Arc<LLMController>,
    from_tui: ToControllerRx,
    cancel_token: CancellationToken,
}

impl InputRouter {
    /// Creates a new input router.
    ///
    /// # Arguments
    /// * `controller` - The LLM controller to forward messages to
    /// * `from_tui` - Channel receiver for messages from the TUI
    /// * `cancel_token` - Token for graceful shutdown
    pub fn new(
        controller: Arc<LLMController>,
        from_tui: ToControllerRx,
        cancel_token: CancellationToken,
    ) -> Self {
        Self {
            controller,
            from_tui,
            cancel_token,
        }
    }

    /// Runs the input router event loop.
    ///
    /// This method will run until either:
    /// - The cancel token is triggered
    /// - The `from_tui` channel is closed
    pub async fn run(mut self) {
        tracing::info!("InputRouter started");

        loop {
            tokio::select! {
                _ = self.cancel_token.cancelled() => {
                    tracing::info!("InputRouter cancelled");
                    break;
                }
                msg = self.from_tui.recv() => {
                    match msg {
                        Some(payload) => {
                            if let Err(e) = self.controller.send_input(payload).await {
                                tracing::error!("Failed to send input to controller: {}", e);
                            }
                        }
                        None => {
                            tracing::info!("TUI channel closed");
                            break;
                        }
                    }
                }
            }
        }

        tracing::info!("InputRouter stopped");
    }
}
