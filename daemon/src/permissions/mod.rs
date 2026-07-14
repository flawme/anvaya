//! Permissions layer.
//!
//! Two responsibilities:
//!   1. **Path policy** — translate a client-supplied path string into an
//!      absolute, canonicalized-under-root path and reject anything that
//!      escapes the configured workspace roots. This is the only gate that
//!      can stop path traversal attacks.
//!   2. **Approval** — a pluggable async [`Approver`] that decides whether a
//!      given action should run. Implementations: [`ConsoleApprover`] (stdin
//!      prompt), [`AutoApprover`] (dev footgun), [`ExtensionApprover`] (waits
//!      for the browser extension popup).

mod approver;
mod policy;

pub use approver::{
    Approval, Approver, AutoApprover, ConsoleApprover, ExtensionApprover, PendingStore,
    PendingSummary,
};
pub use policy::{PathPolicy, ResolveError};
