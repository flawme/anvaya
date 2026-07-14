//! Daemon-level errors mapped to wire [`ResponseError`](anvaya_shared::ResponseError).

use anvaya_shared::{ErrorCode, ResponseError};
use thiserror::Error;

use crate::filesystem::FsError;
use crate::permissions::ResolveError;

#[derive(Debug, Error)]
pub enum DaemonError {
    #[error("invalid request: {0}")]
    Bad(String),
    #[error("path resolution failed: {0}")]
    Resolve(#[from] ResolveError),
    #[error("filesystem error: {0}")]
    Fs(#[from] FsError),
    #[error("request denied by user")]
    Denied,
    #[error("internal error: {0}")]
    Internal(String),
}

impl DaemonError {
    pub fn from_io(e: std::io::Error) -> Self {
        Self::Fs(FsError::Io(e))
    }
}

impl From<DaemonError> for ResponseError {
    fn from(value: DaemonError) -> Self {
        let code = match &value {
            DaemonError::Bad(_) => ErrorCode::BadRequest,
            DaemonError::Resolve(r) => r.code(),
            DaemonError::Fs(FsError::NotFound(_)) => ErrorCode::NotFound,
            DaemonError::Fs(FsError::AlreadyExists(_)) => ErrorCode::Conflict,
            DaemonError::Fs(
                FsError::NotUtf8(_)
                | FsError::NotDir(_)
                | FsError::NotFile(_)
                | FsError::NotEmpty(_)
                | FsError::SamePath,
            ) => ErrorCode::BadRequest,
            DaemonError::Fs(FsError::Io(_)) => ErrorCode::Io,
            DaemonError::Denied => ErrorCode::Forbidden,
            DaemonError::Internal(_) => ErrorCode::Internal,
        };
        ResponseError {
            code,
            message: value.to_string(),
        }
    }
}
