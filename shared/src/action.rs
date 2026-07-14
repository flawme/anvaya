//! Action enum: the closed set of filesystem operations Anvaya supports.
//!
//! Keeping the action set as a single tagged enum (rather than one struct per
//! endpoint) makes exhaustive `match` routing in the daemon trivial and lets
//! the approval layer treat every action uniformly.

use serde::{Deserialize, Serialize};

/// Discriminator for [`Action`]. Cheap to copy, used for metrics/logging.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    Write,
    Mkdir,
    Read,
    List,
    Delete,
    Move,
    Copy,
    Edit,
    Grep,
    Tree,
    Stat,
    ProjectInfo,
    Batch,
    GlobList,
}

impl ActionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Write => "write",
            Self::Mkdir => "mkdir",
            Self::Read => "read",
            Self::List => "list",
            Self::Delete => "delete",
            Self::Move => "move",
            Self::Copy => "copy",
            Self::Edit => "edit",
            Self::Grep => "grep",
            Self::Tree => "tree",
            Self::Stat => "stat",
            Self::ProjectInfo => "project_info",
            Self::Batch => "batch",
            Self::GlobList => "glob_list",
        }
    }
}

/// Whether a write replaces or extends the target file.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WriteLocation {
    #[default]
    Overwrite,
    Append,
}

/// The payload carried by a request. One variant per `ActionKind`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    Write {
        path: String,
        content: String,
        #[serde(default)]
        location: WriteLocation,
    },
    Mkdir {
        path: String,
    },
    Read {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        offset: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        length: Option<usize>,
    },
    List {
        path: String,
    },
    Delete {
        path: String,
    },
    Move {
        src: String,
        dst: String,
    },
    Copy {
        src: String,
        dst: String,
    },
    Edit {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        search: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        replace: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        regex: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        replace_all: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        insert_before: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        insert_after: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        append: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prepend: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dry_run_diff: Option<bool>,
    },
    Grep {
        path: String,
        query: String,
    },
    Tree {
        path: String,
    },
    Stat {
        path: String,
    },
    ProjectInfo {
        path: String,
    },
    Batch {
        actions: Vec<Action>,
    },
    GlobList {
        pattern: String,
    },
}

impl Action {
    pub fn kind(&self) -> ActionKind {
        match self {
            Self::Write { .. } => ActionKind::Write,
            Self::Mkdir { .. } => ActionKind::Mkdir,
            Self::Read { .. } => ActionKind::Read,
            Self::List { .. } => ActionKind::List,
            Self::Delete { .. } => ActionKind::Delete,
            Self::Move { .. } => ActionKind::Move,
            Self::Copy { .. } => ActionKind::Copy,
            Self::Edit { .. } => ActionKind::Edit,
            Self::Grep { .. } => ActionKind::Grep,
            Self::Tree { .. } => ActionKind::Tree,
            Self::Stat { .. } => ActionKind::Stat,
            Self::ProjectInfo { .. } => ActionKind::ProjectInfo,
            Self::Batch { .. } => ActionKind::Batch,
            Self::GlobList { .. } => ActionKind::GlobList,
        }
    }
}

/// Typed result payload returned for a successful action.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActionResponse {
    Write { bytes: usize },
    Mkdir { path: String },
    Delete { removed: usize },
    Move { src: String, dst: String },
    Copy { src: String, dst: String },
    Read(ReadResponse),
    List { entries: Vec<ListEntry> },
    Edit { 
        bytes: usize, 
        matches: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        diff: Option<String>,
    },
    Grep { matches: Vec<GrepMatch> },
    Tree { root: TreeNode },
    Stat { size: u64, modified_secs: u64, is_dir: bool, is_file: bool, is_symlink: bool, unix_mode: Option<u32> },
    ProjectInfo { is_git: bool, language: Option<String>, build_system: Option<String> },
    Batch { responses: Vec<ActionResponse> },
    GlobList { paths: Vec<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrepMatch {
    pub file: String,
    pub line: usize,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeNode {
    pub name: String,
    pub is_dir: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<TreeNode>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadResponse {
    /// UTF-8 decoded contents. Binary files are rejected upstream.
    pub content: String,
    pub bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListEntry {
    pub name: String,
    pub kind: EntryKind,
    pub bytes: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    File,
    Dir,
    Symlink,
}
