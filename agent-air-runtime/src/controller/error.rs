//! Error types for the controller module.

use thiserror::Error;

/// Error type for controller operations.
#[derive(Error, Debug)]
pub enum ControllerError {
    /// Controller has been shutdown.
    #[error("Controller is shutdown")]
    Shutdown,

    /// Channel closed unexpectedly.
    #[error("Channel closed")]
    ChannelClosed,

    /// Send operation timed out.
    #[error("Send timeout after {0} seconds")]
    SendTimeout(u64),
}
