//! Path resolution & traversal protection.

use std::path::{Path, PathBuf};

use thiserror::Error;

use anvaya_shared::ErrorCode;

use crate::models::WorkspaceRoot;

#[derive(Debug, Error)]
pub enum ResolveError {
    /// Supervisor path tried to climb out of every configured root.
    #[error("path escapes all configured workspace roots: {0}")]
    Escapes(String),
    /// The path was empty or otherwise malformed.
    #[error("invalid path: {0}")]
    Invalid(String),
}

impl ResolveError {
    pub fn code(&self) -> ErrorCode {
        match self {
            Self::Escapes(_) => ErrorCode::PathTraversal,
            Self::Invalid(_) => ErrorCode::InvalidPath,
        }
    }
}

/// Resolves client paths against the set of workspace roots.
#[derive(Debug, Clone)]
pub struct PathPolicy {
    roots: Vec<WorkspaceRoot>,
}

impl PathPolicy {
    pub fn new(roots: Vec<WorkspaceRoot>) -> Self {
        Self { roots }
    }

    pub fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }

    /// Resolve `input` to an absolute path that lives under one of the roots.
    ///
    /// `input` may be:
    ///   - relative (`Desktop/x.txt`) → resolved against the *first* root,
    ///   - `~`-prefixed (`~/Desktop/x.txt` or `~/x.txt`) → expanded to the
    ///     user's home directory, then required to live under some root,
    ///   - absolute (`/home/.../Desktop/x.txt`) → must match some root.
    ///
    /// Lexical `..` is collapsed before the roots check, so `../etc/passwd`
    /// can never escape.
    pub fn resolve(&self, input: &str) -> Result<PathBuf, ResolveError> {
        if input.trim().is_empty() {
            return Err(ResolveError::Invalid(input.to_string()));
        }
        // Expand a leading `~` or `~/` to the user's home directory. Agents
        // often write `~/Desktop/x.txt` even when the workspace root already
        // points at $HOME/Desktop; this宽松ens acceptance without weakening
        // the root-containment check.
        let expanded = expand_tilde(input);
        let raw = PathBuf::from(&expanded);
        let candidate = if raw.is_absolute() {
            normalize(&raw)
        } else {
            let base = self
                .roots
                .first()
                .ok_or_else(|| ResolveError::Invalid(input.to_string()))?;
            normalize(&base.abs.join(&raw))
        };

        for root in &self.roots {
            if candidate.starts_with(&root.abs) {
                return Ok(candidate);
            }
        }
        Err(ResolveError::Escapes(input.to_string()))
    }

    /// Resolve two paths (typically `src`/`dst`) in a single call.
    pub fn resolve_pair(&self, src: &str, dst: &str) -> Result<(PathBuf, PathBuf), ResolveError> {
        let s = self.resolve(src)?;
        let d = self.resolve(dst)?;
        Ok((s, d))
    }
}

/// Lexical normalization that collapses `.` and `..` without touching disk.
fn normalize(path: &Path) -> PathBuf {
    let mut out = Vec::new();
    for comp in path.components() {
        use std::path::Component;
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                if let Some(Component::Normal(_)) = out.last() {
                    out.pop();
                }
            }
            c => out.push(c),
        }
    }
    PathBuf::from_iter(out)
}

/// Expand a leading `~` or `~/` to the user's home directory.
/// Returns the input unchanged if `~` is not at the start, or if the home
/// directory cannot be determined.
fn expand_tilde(input: &str) -> String {
    if input == "~" {
        return home().unwrap_or_else(|| input.to_string());
    }
    if let Some(rest) = input.strip_prefix("~/") {
        if let Some(h) = home() {
            return format!("{h}/{rest}");
        }
    }
    input.to_string()
}

fn home() -> Option<String> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()
        .filter(|h| !h.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::WorkspaceRoot;

    fn root(label: &str) -> (WorkspaceRoot, tempfile_capable::Dir) {
        let dir = tempfile_capable::Dir::new();
        let root = WorkspaceRoot::new(&dir.path, label).unwrap();
        (root, dir)
    }

    mod tempfile_capable {
        use std::path::PathBuf;
        pub struct Dir {
            pub path: PathBuf,
        }
        impl Dir {
            pub fn new() -> Self {
                let path = std::env::temp_dir().join(format!(
                    "anvaya-perm-{}-{}",
                    std::process::id(),
                    uuidish()
                ));
                std::fs::create_dir_all(&path).unwrap();
                Self { path }
            }
        }
        fn uuidish() -> u64 {
            use std::time::{SystemTime, UNIX_EPOCH};
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64
        }
        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.path);
            }
        }
    }

    #[test]
    fn relative_resolves_against_first_root() {
        let (root, _d) = root("r");
        let policy = PathPolicy::new(vec![root.clone()]);
        let p = policy.resolve("Desktop/x.txt").unwrap();
        assert!(p.starts_with(&root.abs));
        assert!(p.ends_with("Desktop/x.txt"));
    }

    #[test]
    fn traversal_rejected() {
        let (root, _d) = root("r");
        let policy = PathPolicy::new(vec![root]);
        assert!(matches!(
            policy.resolve("../../etc/passwd"),
            Err(ResolveError::Escapes(_))
        ));
    }

    #[test]
    fn absolute_required_to_match_a_root() {
        let (root, _d) = root("r");
        let inside = root.abs.join("ok.txt");
        let policy = PathPolicy::new(vec![root]);
        // Outside every root.
        assert!(policy.resolve("/etc/passwd").is_err());
        // Inside the root.
        assert!(policy.resolve(&inside.to_string_lossy()).is_ok());
    }

    #[test]
    fn empty_rejected() {
        let (root, _d) = root("r");
        let policy = PathPolicy::new(vec![root]);
        assert!(matches!(
            policy.resolve("  "),
            Err(ResolveError::Invalid(_))
        ));
    }
}
