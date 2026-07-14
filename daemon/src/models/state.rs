//! Shared state installed into the Axum router.

use std::sync::Arc;

use crate::models::WorkspaceRoot;
use crate::permissions::{Approver, PendingStore};

/// Immutable configuration + runtime collaborators, cheaply cloneable.
/// Holds the approval channel so HTTP handlers can reach the pending queue.
#[derive(Clone)]
pub struct AppState {
    inner: Arc<Inner>,
}

struct Inner {
    pub roots: Vec<WorkspaceRoot>,
    pub approver: Box<dyn Approver>,
    pub pending: PendingStore,
}

impl AppState {
    pub fn new(
        roots: Vec<WorkspaceRoot>,
        approver: Box<dyn Approver>,
        pending: PendingStore,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                roots,
                approver,
                pending,
            }),
        }
    }

    pub fn roots(&self) -> &[WorkspaceRoot] {
        &self.inner.roots
    }

    pub fn approver(&self) -> &dyn Approver {
        self.inner.approver.as_ref()
    }

    pub fn pending(&self) -> &PendingStore {
        &self.inner.pending
    }
}
