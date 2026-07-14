//! Daemon configuration.
//!
//! Configuration is loaded from (highest to lowest precedence):
//!   1. environment variables prefixed with `ANVAYA_`
//!   2. an optional `anvaya.toml` next to the binary
//!   3. built-in safe defaults
//!
//! Workspace roots are **deny-by-default**: only explicitly listed roots are
//! reachable. Relative roots are resolved against the current working
//! directory and canonicalized at load time so that path checks are exact.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("no workspace roots configured; refusing to start")]
    NoRoots,
    #[error("workspace root does not exist: {0}")]
    RootMissing(PathBuf),
    #[error("failed to canonicalize workspace root {path}: {source}")]
    Canonicalize {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid bind address: {0}")]
    BadAddress(String),
}

/// A single validated workspace root. `abs` is canonicalized and guaranteed
/// to exist on disk by [`Config::load`].
#[derive(Debug, Clone)]
pub struct WorkspaceRoot {
    /// Canonicalized absolute path.
    pub abs: PathBuf,
    /// Human label used in approval prompts and logs.
    #[allow(dead_code)]
    pub label: String,
}

impl WorkspaceRoot {
    pub fn new(path: impl AsRef<Path>, label: impl Into<String>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(ConfigError::RootMissing(path.to_path_buf()));
        }
        let abs = path
            .canonicalize()
            .map_err(|source| ConfigError::Canonicalize {
                path: path.to_path_buf(),
                source,
            })?;
        Ok(Self {
            abs,
            label: label.into(),
        })
    }

    /// True if `target` lives inside (or equals) this root.
    #[allow(dead_code)]
    pub fn contains(&self, target: &Path) -> bool {
        target.starts_with(&self.abs)
    }
}

/// Daemon configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// `host:port` to bind the HTTP server on. Defaults to `127.0.0.1:7878`.
    #[serde(default = "default_bind")]
    pub bind: String,
    /// Default workspace roots, expressed as filesystem paths.
    #[serde(default)]
    pub roots: Vec<String>,
    /// When true, requests are approved automatically (development only).
    #[serde(default)]
    pub auto_approve: bool,
    /// Maximum request body size in bytes. Defaults to 16 MiB.
    #[serde(default = "default_body_limit")]
    pub body_limit_bytes: usize,
    /// Origin allow-list for CORS. Empty = allow no cross-origin requests.
    #[serde(default)]
    pub allowed_origins: Vec<String>,
}

fn default_bind() -> String {
    "127.0.0.1:7878".to_string()
}

fn default_body_limit() -> usize {
    16 * 1024 * 1024
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind: default_bind(),
            roots: Vec::new(),
            auto_approve: false,
            body_limit_bytes: default_body_limit(),
            allowed_origins: Vec::new(),
        }
    }
}

impl Config {
    /// Build a [`Config`] from the environment, materializing roots.
    ///
    /// Recognized variables:
    /// - `ANVAYA_BIND`            override `bind`
    /// - `ANVAYA_WORKSPACE`       `;`-separated list of roots
    /// - `ANVAYA_AUTO_APPROVE`   `1`/`true` enables auto approval
    /// - `ANVAYA_BODY_LIMIT`       body size limit in bytes
    /// - `ANVAYA_ALLOWED_ORIGINS` `;`-separated origin allow-list
    pub fn from_env() -> Result<Self, ConfigError> {
        let mut cfg = Config::default();
        if let Ok(b) = std::env::var("ANVAYA_BIND") {
            cfg.bind = b;
        }
        if let Ok(ws) = std::env::var("ANVAYA_WORKSPACE") {
            cfg.roots = ws
                .split(';')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(Into::into)
                .collect();
        }
        if let Ok(v) = std::env::var("ANVAYA_AUTO_APPROVE") {
            cfg.auto_approve = matches!(v.as_str(), "1" | "true" | "TRUE" | "yes");
        }
        if let Ok(v) = std::env::var("ANVAYA_BODY_LIMIT") {
            if let Ok(n) = v.parse() {
                cfg.body_limit_bytes = n;
            }
        }
        if let Ok(v) = std::env::var("ANVAYA_ALLOWED_ORIGINS") {
            cfg.allowed_origins = v
                .split(';')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(Into::into)
                .collect();
        }
        cfg.validate()
    }

    /// Resolve and canonicalize configured roots into [`WorkspaceRoot`]s.
    pub fn roots(&self) -> Result<Vec<WorkspaceRoot>, ConfigError> {
        if self.roots.is_empty() {
            return Err(ConfigError::NoRoots);
        }
        self.roots
            .iter()
            .enumerate()
            .map(|(i, r)| WorkspaceRoot::new(r, format!("root-{}", i)))
            .collect()
    }

    fn validate(&self) -> Result<Self, ConfigError> {
        // Fuse the address early to surface config errors before binding.
        if self.bind.parse::<std::net::SocketAddr>().is_err() {
            return Err(ConfigError::BadAddress(self.bind.clone()));
        }
        // Make sure at least one root is configured *and* exists at startup.
        self.roots()?;
        Ok(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_root() -> PathBuf {
        let d = std::env::temp_dir().join(format!("anvaya-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn roots_canonicalized() {
        let root = tmp_root();
        let cfg = Config {
            roots: vec![root.to_string_lossy().to_string()],
            ..Default::default()
        };
        let roots = cfg.roots().unwrap();
        assert_eq!(roots.len(), 1);
        assert!(roots[0].abs.is_absolute());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn empty_roots_rejected() {
        let cfg = Config::default();
        assert!(matches!(cfg.roots(), Err(ConfigError::NoRoots)));
    }

    #[test]
    fn invalid_bind_rejected() {
        let root = tmp_root();
        let cfg = Config {
            bind: "not-an-addr".into(),
            roots: vec![root.to_string_lossy().to_string()],
            ..Default::default()
        };
        assert!(cfg.validate().is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
