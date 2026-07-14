//! Approval gate.
//!
//! Every [`Request`](anvaya_shared::Request) is shown to an [`Approver`] and
//! only executed once the approver returns [`Approval::Allow`]. Implementations:
//!   - [`ConsoleApprover`] prompts on stdin (blocking, default).
//!   - [`AutoApprover`] blindly allows — dev only (`ANVAYA_AUTO_APPROVE=1`).
//!   - [`ExtensionApprover`](crate::permissions::ExtensionApprover) blocks until
//!     the browser extension popup calls `/api/v1/approve|deny`.

use std::io::{self, BufRead, Write};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::oneshot;

use anvaya_shared::Request;

/// The approver's decision for a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Approval {
    Allow,
    Deny,
}

/// Async, object-safe approval surface. Implementations may block for arbitrary
/// durations (e.g. wait for the user to click a button in a browser window);
/// the actions layer awaits this future before executing.
#[async_trait]
pub trait Approver: Send + Sync + 'static {
    async fn approve(&self, req: &Request) -> Approval;
}

/// Development-only approver that allows everything.
pub struct AutoApprover;

#[async_trait]
impl Approver for AutoApprover {
    async fn approve(&self, _req: &Request) -> Approval {
        Approval::Allow
    }
}

/// Prompts the operator on stdout/stdin for each request. Strings `y`/`yes`
/// allow; `a`/`always` allows for the rest of the process; anything else denies.
pub struct ConsoleApprover {
    always: Arc<Mutex<bool>>,
}

impl ConsoleApprover {
    pub fn new() -> Self {
        Self {
            always: Arc::new(Mutex::new(false)),
        }
    }
}

impl Default for ConsoleApprover {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Approver for ConsoleApprover {
    async fn approve(&self, req: &Request) -> Approval {
        if *self.always.lock().unwrap() {
            return Approval::Allow;
        }

        let summary = summarize(req);
        let prompt =
            format!("\n[anvaya] approval requested:\n  {summary}\nAllow? [y/N/a=always]: ");

        // stdin is blocking; keep the async thread pool free.
        let always_flag = self.always.clone();
        let mobi_always = Arc::new(Mutex::new(false));
        let inner_always = mobi_always.clone();
        let result = tokio::task::spawn_blocking(move || -> Approval {
            let stdout = io::stdout();
            let mut out = stdout.lock();
            let _ = out.write_all(prompt.as_bytes());
            let _ = out.flush();
            let stdin = io::stdin();
            let mut line = String::new();
            if stdin.lock().read_line(&mut line).is_err() {
                return Approval::Deny;
            }
            match line.trim().to_ascii_lowercase().as_str() {
                "y" | "yes" => Approval::Allow,
                "a" | "always" => {
                    *inner_always.lock().unwrap() = true;
                    Approval::Allow
                }
                _ => Approval::Deny,
            }
        })
        .await;

        if *mobi_always.lock().unwrap() {
            // Persist the choice for the remainder of the process.
            *always_flag.lock().unwrap() = true;
        }
        result.unwrap_or(Approval::Deny)
    }
}

/// Approver that forwards every decision to the browser extension via the
/// [`PendingStore`]. When `approve` is called, the request is enqueued and the
/// future suspends until the extension calls `/api/v1/approve` or `/deny`.
pub struct ExtensionApprover {
    pub store: PendingStore,
}

#[async_trait]
impl Approver for ExtensionApprover {
    async fn approve(&self, req: &Request) -> Approval {
        let (tx, rx) = oneshot::channel();
        self.store.insert(req.id, req.clone(), tx).await;
        match rx.await {
            Ok(approval) => approval,
            Err(_) => Approval::Deny,
        }
    }
}

/// In-memory store of requests awaiting an approval decision from the
/// extension. Cheaply cloneable via the inner `Arc`.
#[derive(Clone)]
pub struct PendingStore {
    inner: std::sync::Arc<tokio::sync::Mutex<PendingInner>>,
}

#[derive(Default)]
struct PendingInner {
    /// request id → (summary, oneshot sender).
    items: std::collections::HashMap<
        uuid::Uuid,
        (anvaya_shared::Request, Option<oneshot::Sender<Approval>>),
    >,
}

/// Serializable summary of a pending request, returned by `/api/v1/pending`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PendingSummary {
    pub id: uuid::Uuid,
    pub kind: String,
    pub summary: String,
    pub reason: Option<String>,
}

impl PendingStore {
    pub fn new() -> Self {
        Self {
            inner: std::sync::Arc::new(tokio::sync::Mutex::new(PendingInner::default())),
        }
    }

    pub async fn insert(
        &self,
        id: uuid::Uuid,
        req: anvaya_shared::Request,
        tx: oneshot::Sender<Approval>,
    ) {
        self.inner.lock().await.items.insert(id, (req, Some(tx)));
    }

    pub async fn list(&self) -> Vec<PendingSummary> {
        self.inner
            .lock()
            .await
            .items
            .iter()
            .map(|(id, (req, _))| PendingSummary {
                id: *id,
                kind: req.kind.as_str().to_string(),
                summary: summarize(req),
                reason: req.reason.clone(),
            })
            .collect()
    }

    pub async fn decide(&self, id: uuid::Uuid, approval: Approval) -> bool {
        if let Some((_, tx_opt)) = self.inner.lock().await.items.remove(&id) {
            if let Some(tx) = tx_opt {
                let _ = tx.send(approval);
            }
            true
        } else {
            false
        }
    }

    /// Drop everything pending (used for tests).
    #[allow(dead_code)]
    #[cfg(test)]
    pub async fn clear(&self) {
        self.inner.lock().await.items.clear();
    }
}

impl Default for PendingStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Human-readable one-liner that shows what the agent is asking for.
fn summarize(req: &Request) -> String {
    use anvaya_shared::Action;
    match &req.action {
        Action::Write {
            path,
            content,
            location,
        } => {
            let loc = if *location == anvaya_shared::WriteLocation::Append {
                "append"
            } else {
                "overwrite"
            };
            format!("write ({loc}, {} bytes) -> {path}", content.len())
        }
        Action::Mkdir { path } => format!("mkdir -> {path}"),
        Action::Read { path, .. } => format!("read <- {path}"),
        Action::List { path } => format!("list <- {path}"),
        Action::Delete { path } => format!("delete ! {path}"),
        Action::Move { src, dst } => format!("move {src} -> {dst}"),
        Action::Copy { src, dst } => format!("copy {src} -> {dst}"),
        Action::Edit {
            path,
            search,
            replace,
            ..
        } => {
            format!(
                "edit {path} ({} -> {})",
                truncate(search.as_deref().unwrap_or(""), 40),
                truncate(replace.as_deref().unwrap_or(""), 40)
            )
        }
        Action::Grep { path, query } => {
            format!("grep {} -> {path}", truncate(query, 40))
        }
        Action::Tree { path } => format!("tree <- {path}"),
        Action::Stat { path } => format!("stat <- {path}"),
        Action::ProjectInfo { path } => format!("project_info <- {path}"),
        Action::Batch { actions } => format!("batch ({} actions)", actions.len()),
        Action::GlobList { pattern } => format!("glob_list <- {pattern}"),
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(n).collect();
        t.push('…');
        t
    }
}
