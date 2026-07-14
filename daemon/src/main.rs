//! Anvaya daemon entry point.
//!
//! Loads configuration, wires the approval gate, and serves the local HTTP
//! API. See `README.md` for usage and `SECURITY.md` for the threat model.
//!
//! Quick start:
//!   ANVAYA_WORKSPACE=$HOME/Desktop ANVAYA_BIND=127.0.0.1:7878 cargo run -p anvaya-daemon
//! and (for local dev only):
//!   ANVAYA_AUTO_APPROVE=1 cargo run -p anvaya-daemon   # WARNING: skips prompts

use anyhow::Context;
use tracing_subscriber::EnvFilter;

mod actions;
mod api;
mod filesystem;
mod models;
mod permissions;
mod server;

use models::{AppState, Config};
use permissions::{Approver, AutoApprover, ConsoleApprover, ExtensionApprover, PendingStore};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cfg = Config::from_env().context("failed to load configuration")?;
    let roots = cfg.roots().context("failed to resolve workspace roots")?;

    tracing::info!(
        bind = %cfg.bind,
        auto_approve = cfg.auto_approve,
        approval = if cfg.auto_approve { "auto" } else { "extension" },
        roots = ?roots.iter().map(|r| r.abs.as_path()).collect::<Vec<_>>(),
        "starting anvaya daemon"
    );

    if cfg.auto_approve {
        tracing::warn!("AUTO_APPROVE is enabled — every request runs without a prompt");
    }

    let pending = PendingStore::new();
    let approver: Box<dyn Approver> = if cfg.auto_approve {
        Box::new(AutoApprover)
    } else {
        Box::new(ExtensionApprover {
            store: pending.clone(),
        })
    };
    // Sanity: ConsoleApprover is available as a fallback for headless use.
    let _ = ConsoleApprover::new;
    let state = AppState::new(roots, approver, pending);

    server::serve(state, &cfg).await
}
