//! Action dispatch: resolve → approve → execute.
//!
//! This is the single chokepoint where every request flows through, in this
//! exact order:
//!   1. The path policy resolves and validates the client paths.
//!   2. The approver gets a chance to deny the request.
//!   3. The filesystem executes the action.
//!
//! No other code path is permitted to touch the filesystem.

use anvaya_shared::{Action, ActionResponse, Request};

use crate::filesystem::FileSystem;
use crate::models::AppState;
use crate::models::DaemonError;
use crate::permissions::{Approval, ResolveError};

/// Execute a fully-decoded [`Request`] against `state`.
pub async fn execute(req: &Request, state: &AppState) -> Result<ActionResponse, DaemonError> {
    let policy = crate::permissions::PathPolicy::new(state.roots().to_vec());
    if policy.is_empty() {
        return Err(DaemonError::Internal(
            "no workspace roots configured".into(),
        ));
    }

    // 1. Resolve paths first. Denying here is cheap and never touches disk.
    let resolvable = resolve_paths(&policy, &req.action)?;

    // 2. Ask the approver (may prompt the user or the extension).
    match state.approver().approve(req).await {
        Approval::Allow => {}
        Approval::Deny => return Err(DaemonError::Denied),
    }

    // 3. Execute on the filesystem.
    let fs = FileSystem;
    execute_action(&req.action, resolvable, &fs).await
}

#[async_recursion::async_recursion]
async fn execute_action(action: &Action, resolvable: Resolved, fs: &FileSystem) -> Result<ActionResponse, DaemonError> {
    let resp = match action {
        Action::Write {
            content, location, ..
        } => {
            let Resolved::One(path) = resolvable else {
                return Err(DaemonError::Internal("path/server mismatch".into()));
            };
            let bytes = fs.write(&path, content, *location).await?;
            ActionResponse::Write { bytes }
        }
        Action::Mkdir { .. } => {
            let Resolved::One(path) = resolvable else {
                return Err(DaemonError::Internal("path/server mismatch".into()));
            };
            fs.mkdir(&path).await?;
            ActionResponse::Mkdir {
                path: path.to_string_lossy().to_string(),
            }
        }
        Action::Read { offset, length, .. } => {
            let Resolved::One(path) = resolvable else {
                return Err(DaemonError::Internal("path/server mismatch".into()));
            };
            let (content, bytes) = fs.read(&path, *offset, *length).await?;
            ActionResponse::Read(anvaya_shared::ReadResponse { content, bytes })
        }
        Action::List { .. } => {
            let Resolved::One(path) = resolvable else {
                return Err(DaemonError::Internal("path/server mismatch".into()));
            };
            let entries = fs.list(&path).await?;
            ActionResponse::List { entries }
        }
        Action::Delete { .. } => {
            let Resolved::One(path) = resolvable else {
                return Err(DaemonError::Internal("path/server mismatch".into()));
            };
            let removed = fs.delete(&path).await?;
            ActionResponse::Delete { removed }
        }
        Action::Move { .. } => {
            let Resolved::Pair(s, d) = resolvable else {
                return Err(DaemonError::Internal("path/server mismatch".into()));
            };
            fs.mv(&s, &d).await?;
            ActionResponse::Move {
                src: s.to_string_lossy().to_string(),
                dst: d.to_string_lossy().to_string(),
            }
        }
        Action::Copy { .. } => {
            let Resolved::Pair(s, d) = resolvable else {
                return Err(DaemonError::Internal("path/server mismatch".into()));
            };
            fs.cp(&s, &d).await?;
            ActionResponse::Copy {
                src: s.to_string_lossy().to_string(),
                dst: d.to_string_lossy().to_string(),
            }
        }
        Action::Edit {
            search, replace, regex, replace_all, insert_before, insert_after, append, prepend, dry_run_diff, ..
        } => {
            let Resolved::One(path) = resolvable else {
                return Err(DaemonError::Internal("path/server mismatch".into()));
            };
            let (bytes, matches, diff) = fs.edit(
                &path, 
                search.as_deref(), 
                replace.as_deref(), 
                *regex, 
                *replace_all, 
                insert_before.as_deref(), 
                insert_after.as_deref(), 
                *append, 
                *prepend, 
                *dry_run_diff
            ).await?;
            ActionResponse::Edit { bytes, matches, diff }
        }
        Action::Grep { query, .. } => {
            let Resolved::One(path) = resolvable else {
                return Err(DaemonError::Internal("path/server mismatch".into()));
            };
            let matches = fs.grep(&path, query).await?;
            ActionResponse::Grep { matches }
        }
        Action::Tree { .. } => {
            let Resolved::One(path) = resolvable else {
                return Err(DaemonError::Internal("path/server mismatch".into()));
            };
            let root = fs.tree(&path).await?;
            ActionResponse::Tree { root }
        }
        Action::Stat { .. } => {
            let Resolved::One(path) = resolvable else {
                return Err(DaemonError::Internal("path/server mismatch".into()));
            };
            let (size, modified_secs, is_dir, is_file, is_symlink, unix_mode) = fs.stat(&path).await?;
            ActionResponse::Stat { size, modified_secs, is_dir, is_file, is_symlink, unix_mode }
        }
        Action::ProjectInfo { .. } => {
            let Resolved::One(path) = resolvable else {
                return Err(DaemonError::Internal("path/server mismatch".into()));
            };
            let (is_git, language, build_system) = fs.project_info(&path).await?;
            ActionResponse::ProjectInfo { is_git, language, build_system }
        }
        Action::Batch { actions } => {
            let Resolved::Batch(resolved_actions) = resolvable else {
                return Err(DaemonError::Internal("path/server mismatch".into()));
            };
            let mut responses = Vec::new();
            for (act, res) in actions.iter().zip(resolved_actions.into_iter()) {
                let resp = execute_action(act, res, fs).await?;
                responses.push(resp);
            }
            ActionResponse::Batch { responses }
        }
        Action::GlobList { .. } => {
            let Resolved::One(path) = resolvable else {
                return Err(DaemonError::Internal("path/server mismatch".into()));
            };
            let paths = fs.glob_list(&path).await?;
            ActionResponse::GlobList { paths }
        }
    };

    Ok(resp)
}

/// Pre-resolved and validated paths for an action.
enum Resolved {
    One(std::path::PathBuf),
    Pair(std::path::PathBuf, std::path::PathBuf),
    Batch(Vec<Resolved>),
}

fn resolve_paths(
    policy: &crate::permissions::PathPolicy,
    action: &Action,
) -> Result<Resolved, ResolveError> {
    Ok(match action {
        Action::Write { path, .. }
        | Action::Mkdir { path }
        | Action::Read { path }
        | Action::List { path }
        | Action::Delete { path }
        | Action::Edit { path, .. }
        | Action::Grep { path, .. }
        | Action::Tree { path }
        | Action::Stat { path }
        | Action::ProjectInfo { path } => Resolved::One(policy.resolve(path)?),
        Action::GlobList { pattern } => Resolved::One(policy.resolve(pattern)?),
        Action::Batch { actions } => {
            let mut resolved = Vec::new();
            for act in actions {
                resolved.push(resolve_paths(policy, act)?);
            }
            Resolved::Batch(resolved)
        }
        Action::Move { src, dst } | Action::Copy { src, dst } => {
            let (s, d) = policy.resolve_pair(src, dst)?;
            Resolved::Pair(s, d)
        }
    })
}
