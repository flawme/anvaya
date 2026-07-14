//! Shared protocol types between Anvaya components.
//!
//! Request envelope sent by the browser extension (`Request`), helper constructors
//! for each action (`Request::write`, `Request::mkdir`, ...), and the typed
//! `Response` returned by the daemon.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub mod action;

pub use action::{
    Action, ActionKind, ActionResponse, EntryKind, ListEntry, ReadResponse, WriteLocation,
    GrepMatch, TreeNode,
};

/// API wire version. Bumped on breaking protocol changes.
pub const PROTOCOL_VERSION: u32 = 1;
/// API path prefix.
pub const API_PREFIX: &str = "/api/v1";

/// A single filesystem request issued by an AI / extension.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    /// Client-generated id; echoed back in the response.
    pub id: Uuid,
    /// Logical kind of action; drives routing on the daemon side.
    pub kind: ActionKind,
    /// Actual action payload.
    pub action: action::Action,
    /// Optional human-readable reason supplied by the agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl Request {
    pub fn new(kind: ActionKind, action: action::Action) -> Self {
        Self {
            id: Uuid::new_v4(),
            kind,
            action,
            reason: None,
        }
    }

    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    pub fn mkdir(path: impl Into<String>) -> Self {
        Self::new(
            ActionKind::Mkdir,
            action::Action::Mkdir { path: path.into() },
        )
    }

    pub fn write(path: impl Into<String>, content: String, location: WriteLocation) -> Self {
        Self::new(
            ActionKind::Write,
            action::Action::Write {
                path: path.into(),
                content,
                location,
            },
        )
    }

    pub fn read(path: impl Into<String>) -> Self {
        Self::new(ActionKind::Read, action::Action::Read { path: path.into(), offset: None, length: None })
    }

    pub fn list(path: impl Into<String>) -> Self {
        Self::new(ActionKind::List, action::Action::List { path: path.into() })
    }

    pub fn delete(path: impl Into<String>) -> Self {
        Self::new(
            ActionKind::Delete,
            action::Action::Delete { path: path.into() },
        )
    }

    pub fn mv(src: impl Into<String>, dst: impl Into<String>) -> Self {
        Self::new(
            ActionKind::Move,
            action::Action::Move {
                src: src.into(),
                dst: dst.into(),
            },
        )
    }

    pub fn cp(src: impl Into<String>, dst: impl Into<String>) -> Self {
        Self::new(
            ActionKind::Copy,
            action::Action::Copy {
                src: src.into(),
                dst: dst.into(),
            },
        )
    }
}

/// Daemon response envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub id: Uuid,
    pub status: ResponseStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<action::ActionResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ResponseError>,
}

impl Response {
    pub fn ok(id: Uuid, result: action::ActionResponse) -> Self {
        Self {
            id,
            status: ResponseStatus::Ok,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: Uuid, code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            id,
            status: ResponseStatus::Error,
            result: None,
            error: Some(ResponseError {
                code,
                message: message.into(),
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResponseStatus {
    Ok,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseError {
    pub code: ErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    BadRequest,
    Unauthorized,
    Forbidden,
    NotFound,
    Conflict,
    InvalidPath,
    PathTraversal,
    Io,
    Internal,
}

/// Resolve a possibly-relative client path against a workspace root.
///
/// The returned path is canonicalized-safe: it never escapes `root`.
pub fn resolve_under(root: &std::path::Path, input: &str) -> Result<PathBuf, &'static str> {
    let p = PathBuf::from(input);
    let joined = if p.is_absolute() {
        // Allow absolute paths but they must still live under root.
        p
    } else {
        root.join(p)
    };
    let normalized = normalize(&joined);
    if !normalized.starts_with(root) {
        return Err("path escapes workspace root");
    }
    Ok(normalized)
}

/// Lexical path normalization that collapses `.` and `..` without touching disk.
fn normalize(path: &std::path::Path) -> PathBuf {
    let mut out = Vec::new();
    for comp in path.components() {
        use std::path::Component;
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(out.last(), Some(Component::Normal(_))) {
                    out.pop();
                }
            }
            c => out.push(c),
        }
    }
    PathBuf::from_iter(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_relative_under_root() {
        let root = PathBuf::from("/tmp/anvaya");
        let r = resolve_under(&root, "Desktop/test.txt").unwrap();
        assert_eq!(r, PathBuf::from("/tmp/anvaya/Desktop/test.txt"));
    }

    #[test]
    fn reject_traversal() {
        let root = PathBuf::from("/tmp/anvaya");
        assert!(resolve_under(&root, "../../etc/passwd").is_err());
    }

    #[test]
    fn reject_absolute_outside_root() {
        let root = PathBuf::from("/tmp/anvaya");
        assert!(resolve_under(&root, "/etc/passwd").is_err());
    }

    #[test]
    fn allow_absolute_inside_root() {
        let root = PathBuf::from("/tmp/anvaya");
        let r = resolve_under(&root, "/tmp/anvaya/Desktop/test.txt").unwrap();
        assert_eq!(r, PathBuf::from("/tmp/anvaya/Desktop/test.txt"));
    }
}
