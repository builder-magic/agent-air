//! Optional embedded database backed by LMDB.
//!
//! Enable with the `db` Cargo feature. Call
//! [`AgentAir::enable_database`](crate::agent::AgentAir::enable_database)
//! to initialize.

mod config;
mod env;
mod error;
mod store;

pub use config::DbConfig;
pub use env::AgentDatabase;
pub use error::DbError;
pub use store::Store;
