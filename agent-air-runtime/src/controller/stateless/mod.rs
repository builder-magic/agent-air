//! Stateless LLM execution without session management.

mod executor;
mod types;

pub use executor::StatelessExecutor;
pub use types::{
    DEFAULT_MAX_TOKENS, RequestOptions, StatelessConfig, StatelessError, StatelessResult,
    StreamCallback,
};
