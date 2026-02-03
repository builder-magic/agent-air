//! Stateless LLM execution without session management.

mod executor;
mod types;

pub use executor::StatelessExecutor;
pub use types::{
    RequestOptions, StatelessConfig, StatelessError, StatelessResult, StreamCallback,
    DEFAULT_MAX_TOKENS,
};
