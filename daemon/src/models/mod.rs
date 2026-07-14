//! Daemon models: configuration and shared application state.

mod config;
mod error;
mod state;

#[allow(unused_imports)]
pub use config::{Config, ConfigError, WorkspaceRoot};
pub use error::DaemonError;
pub use state::AppState;
