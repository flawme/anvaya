//! Low-level filesystem operations.
//!
//! Every function here operates exclusively via `std::fs` / `tokio::fs`.
//! There is intentionally **no** way to spawn a process or run a shell command
//! from this module; that is a hard security boundary of Anvaya.

pub mod ops;

pub use ops::{FileSystem, FsError};
