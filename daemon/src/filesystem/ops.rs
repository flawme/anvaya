//! Concrete filesystem primitives backing every action.
//!
//! All calls are `async` and perform blocking I/O on a Tokio blocking thread
//! via [`tokio::fs`] / [`tokio::task::spawn_blocking`] for canonicalization
//! and recursive deletes, so the async runtime is never stalled.

use std::path::{Path, PathBuf};

use anvaya_shared::{EntryKind, ListEntry, WriteLocation, GrepMatch, TreeNode};
use thiserror::Error;
use tokio::fs;

/// Filesystem-level error, mapped to a wire [`ErrorCode`](anvaya_shared::ErrorCode)
/// by the actions layer.
#[derive(Debug, Error)]
pub enum FsError {
    #[error("not found: {0}")]
    NotFound(PathBuf),
    #[error("already exists: {0}")]
    AlreadyExists(PathBuf),
    #[error("invalid UTF-8 in file: {0}")]
    NotUtf8(PathBuf),
    #[error("path is not a directory: {0}")]
    NotDir(PathBuf),
    #[error("path is not a file: {0}")]
    NotFile(PathBuf),
    #[error("directory not empty (recursive delete not allowed for root): {0}")]
    NotEmpty(PathBuf),
    #[error("source and destination are identical")]
    SamePath,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// A handle to the (stateless) filesystem. Methods take already-resolved,
/// policy-checked absolute paths.
#[derive(Debug, Default, Clone, Copy)]
pub struct FileSystem;

/// Summary used by list entries.
#[derive(Debug, Clone, Copy)]
enum EntrySummary {
    File(u64),
    Dir,
    Symlink,
}

impl EntrySummary {
    fn into_entry(self, name: String) -> ListEntry {
        match self {
            Self::File(bytes) => ListEntry {
                name,
                kind: EntryKind::File,
                bytes,
            },
            Self::Dir => ListEntry {
                name,
                kind: EntryKind::Dir,
                bytes: 0,
            },
            Self::Symlink => ListEntry {
                name,
                kind: EntryKind::Symlink,
                bytes: 0,
            },
        }
    }
}

impl FileSystem {
    /// Write `content` to `path`, creating parent dirs as needed.
    pub async fn write(
        &self,
        path: &Path,
        content: &str,
        location: WriteLocation,
    ) -> Result<usize, FsError> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).await?;
            }
        }

        let bytes = content.len();
        match location {
            WriteLocation::Overwrite => fs::write(path, content.as_bytes()).await?,
            WriteLocation::Append => {
                use tokio::io::AsyncWriteExt;
                let mut f = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .await?;
                f.write_all(content.as_bytes()).await?;
            }
        }
        Ok(bytes)
    }

    /// Create a directory (and intermediate parents). Idempotent: ok if it
    /// already exists.
    pub async fn mkdir(&self, path: &Path) -> Result<(), FsError> {
        match fs::create_dir_all(path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    /// Read a file as UTF-8.
    pub async fn read(&self, path: &Path, offset: Option<usize>, length: Option<usize>) -> Result<(String, usize), FsError> {
        if !path.is_file() {
            // Re-check in case it was a directory or fifo.
            let meta = fs::metadata(path).await?;
            if meta.is_dir() {
                return Err(FsError::NotFile(path.to_path_buf()));
            }
        }
        
        let mut f = fs::File::open(path).await?;
        let meta = f.metadata().await?;
        let size = meta.len() as usize;
        
        let start = offset.unwrap_or(0).min(size);
        let end = if let Some(len) = length {
            (start + len).min(size)
        } else {
            size
        };
        let read_len = end - start;
        
        if start > 0 {
            use tokio::io::AsyncSeekExt;
            f.seek(std::io::SeekFrom::Start(start as u64)).await?;
        }
        
        let mut buf = vec![0u8; read_len];
        use tokio::io::AsyncReadExt;
        f.read_exact(&mut buf).await?;
        
        match String::from_utf8(buf) {
            Ok(s) => Ok((s, read_len)),
            Err(_) => Err(FsError::NotUtf8(path.to_path_buf())),
        }
    }
    /// List directory entries, sorted by name.
    pub async fn list(&self, path: &Path) -> Result<Vec<ListEntry>, FsError> {
        let meta = fs::metadata(path).await;
        match meta {
            Ok(m) if m.is_dir() => {}
            Ok(_) => return Err(FsError::NotDir(path.to_path_buf())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(FsError::NotFound(path.to_path_buf()));
            }
            Err(e) => return Err(e.into()),
        }

        let mut entries = fs::read_dir(path).await?;
        let mut out = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name().to_string_lossy().to_string();
            let ft = entry.file_type().await?;
            let summary = if ft.is_symlink() {
                EntrySummary::Symlink
            } else if ft.is_dir() {
                EntrySummary::Dir
            } else {
                let len = entry.metadata().await?.len();
                EntrySummary::File(len)
            };
            out.push(summary.into_entry(name));
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    /// Delete a file or directory tree. Returns the number of entries removed
    /// (1 for a single file, ≥1 for a tree).
    pub async fn delete(&self, path: &Path) -> Result<usize, FsError> {
        let meta = match fs::symlink_metadata(path).await {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(FsError::NotFound(path.to_path_buf()));
            }
            Err(e) => return Err(e.into()),
        };

        if meta.is_dir() {
            // spawn_blocking: recursive removal can be expensive.
            let path = path.to_path_buf();
            let count = tokio::task::spawn_blocking(move || -> Result<usize, std::io::Error> {
                let count = count_recursive(&path)?;
                std::fs::remove_dir_all(&path)?;
                Ok(count)
            })
            .await
            .map_err(|e| FsError::Io(std::io::Error::other(e)))??;
            Ok(count)
        } else {
            fs::remove_file(path).await?;
            Ok(1)
        }
    }

    /// Move/rename `src` to `dst`. Falls back to copy+delete across volumes.
    pub async fn mv(&self, src: &Path, dst: &Path) -> Result<(), FsError> {
        if fs::canonicalize(src).await.ok().as_deref()
            == fs::canonicalize(dst).await.ok().as_deref()
        {
            return Err(FsError::SamePath);
        }
        match fs::rename(src, dst).await {
            Ok(()) => Ok(()),
            // cross-device link → copy then remove
            Err(e)
                if e.raw_os_error() == Some(18) /* EXDEV */ =>
            {
                self.copy_inner(src, dst).await?;
                self.delete(src).await?;
                Ok(())
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Copy `src` to `dst`, recursively if `src` is a directory.
    pub async fn cp(&self, src: &Path, dst: &Path) -> Result<(), FsError> {
        if fs::canonicalize(src).await.ok().as_deref()
            == fs::canonicalize(dst).await.ok().as_deref()
        {
            return Err(FsError::SamePath);
        }
        self.copy_inner(src, dst).await
    }

    /// Replace occurrences, insert text, or generate diffs.
    pub async fn edit(
        &self,
        path: &Path,
        search: Option<&str>,
        replace: Option<&str>,
        regex: Option<bool>,
        replace_all: Option<bool>,
        insert_before: Option<&str>,
        insert_after: Option<&str>,
        append: Option<bool>,
        prepend: Option<bool>,
        dry_run_diff: Option<bool>,
    ) -> Result<(usize, usize, Option<String>), FsError> {
        let (content, _) = self.read(path, None, None).await?;
        let mut new_content = content.clone();
        let mut matches = 0;

        if append.unwrap_or(false) {
            if let Some(r) = replace {
                new_content.push_str(r);
                matches += 1;
            }
        } else if prepend.unwrap_or(false) {
            if let Some(r) = replace {
                new_content.insert_str(0, r);
                matches += 1;
            }
        } else if let (Some(s), Some(r)) = (search, replace) {
            if regex.unwrap_or(false) {
                let re = regex::Regex::new(s).map_err(|e| FsError::Io(std::io::Error::other(e)))?;
                matches = re.find_iter(&content).count();
                if matches > 0 {
                    if replace_all.unwrap_or(true) {
                        new_content = re.replace_all(&content, r).to_string();
                    } else {
                        new_content = re.replace(&content, r).to_string();
                        matches = 1; 
                    }
                }
            } else {
                matches = content.matches(s).count();
                if matches > 0 {
                    if replace_all.unwrap_or(true) {
                        new_content = content.replace(s, r);
                    } else {
                        new_content = content.replacen(s, r, 1);
                        matches = 1;
                    }
                }
            }
        } else if let (Some(b), Some(r)) = (insert_before, replace) {
            if let Some(pos) = content.find(b) {
                new_content.insert_str(pos, r);
                matches = 1;
            }
        } else if let (Some(a), Some(r)) = (insert_after, replace) {
            if let Some(pos) = content.find(a) {
                new_content.insert_str(pos + a.len(), r);
                matches = 1;
            }
        }

        if matches == 0 && !append.unwrap_or(false) && !prepend.unwrap_or(false) {
            return Err(FsError::NotFound(path.to_path_buf()));
        }

        let diff = if dry_run_diff.unwrap_or(false) {
            let diff_str = similar::TextDiff::from_lines(&content, &new_content)
                .unified_diff()
                .header("original", "modified")
                .to_string();
            Some(diff_str)
        } else {
            None
        };

        let bytes = new_content.len();
        if !dry_run_diff.unwrap_or(false) {
            self.write(path, &new_content, WriteLocation::Overwrite).await?;
        }

        Ok((bytes, matches, diff))
    }

    pub async fn grep(&self, path: &Path, query: &str) -> Result<Vec<GrepMatch>, FsError> {
        let path = path.to_path_buf();
        let query = query.to_string();
        tokio::task::spawn_blocking(move || {
            let mut matches = Vec::new();
            let builder = ignore::WalkBuilder::new(&path);
            for result in builder.build() {
                let entry = match result {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                if entry.file_type().is_none_or(|ft| ft.is_dir()) {
                    continue;
                }
                let file_path = entry.path();
                let Ok(content) = std::fs::read_to_string(file_path) else {
                    continue;
                };
                let rel_path = file_path.strip_prefix(&path).unwrap_or(file_path).to_string_lossy().to_string();
                for (i, line) in content.lines().enumerate() {
                    if line.contains(&query) {
                        matches.push(GrepMatch {
                            file: rel_path.clone(),
                            line: i + 1,
                            content: line.to_string(),
                        });
                    }
                }
            }
            Ok(matches)
        }).await.map_err(|e| FsError::Io(std::io::Error::other(e)))?
    }

    pub async fn tree(&self, path: &Path) -> Result<TreeNode, FsError> {
        let path = path.to_path_buf();
        let is_dir = fs::metadata(&path).await.map(|m| m.is_dir()).unwrap_or(false);
        if !is_dir {
            return Err(FsError::NotDir(path));
        }
        
        tokio::task::spawn_blocking(move || {
            let mut iter = ignore::WalkBuilder::new(&path).build();
            let _ = iter.next(); // skip root
            
            #[derive(Default)]
            struct BuildNode {
                is_dir: bool,
                children: std::collections::BTreeMap<String, BuildNode>,
            }
            let mut root_build = BuildNode { is_dir: true, children: Default::default() };
            
            for result in iter {
                let entry = match result {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                let entry_path = entry.path();
                if let Ok(rel) = entry_path.strip_prefix(&path) {
                    let mut current = &mut root_build;
                    let components: Vec<_> = rel.iter().map(|s| s.to_string_lossy().into_owned()).collect();
                    for (i, comp) in components.iter().enumerate() {
                        if i == components.len() - 1 {
                            let is_dir = entry.file_type().is_some_and(|ft| ft.is_dir());
                            let node = current.children.entry(comp.clone()).or_insert_with(|| BuildNode {
                                is_dir,
                                children: Default::default(),
                            });
                            node.is_dir = is_dir;
                        } else {
                            current = current.children.entry(comp.clone()).or_insert_with(|| BuildNode {
                                is_dir: true,
                                children: Default::default(),
                            });
                        }
                    }
                }
            }
            
            fn to_tree_node(name: String, build_node: BuildNode) -> TreeNode {
                let children = if build_node.is_dir {
                    Some(build_node.children.into_iter().map(|(k, v)| to_tree_node(k, v)).collect())
                } else {
                    None
                };
                TreeNode {
                    name,
                    is_dir: build_node.is_dir,
                    children,
                }
            }
            
            Ok(to_tree_node(".".to_string(), root_build))
        }).await.map_err(|e| FsError::Io(std::io::Error::other(e)))?
    }

    pub async fn stat(&self, path: &Path) -> Result<(u64, u64, bool, bool, bool, Option<u32>), FsError> {
        let meta_sym = match fs::symlink_metadata(path).await {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(FsError::NotFound(path.to_path_buf())),
            Err(e) => return Err(e.into()),
        };
        
        let is_symlink = meta_sym.is_symlink();
        
        let meta = if is_symlink {
            fs::metadata(path).await.unwrap_or(meta_sym.clone())
        } else {
            meta_sym.clone()
        };

        let size = meta.len();
        let modified_secs = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
        let is_dir = meta.is_dir();
        let is_file = meta.is_file();
        
        #[cfg(unix)]
        let unix_mode = {
            use std::os::unix::fs::PermissionsExt;
            Some(meta.permissions().mode())
        };
        #[cfg(not(unix))]
        let unix_mode = None;
        
        Ok((size, modified_secs, is_dir, is_file, is_symlink, unix_mode))
    }

    pub async fn project_info(&self, path: &Path) -> Result<(bool, Option<String>, Option<String>), FsError> {
        let is_git = fs::metadata(path.join(".git")).await.is_ok();
        
        let mut language = None;
        let mut build_system = None;
        
        if fs::metadata(path.join("Cargo.toml")).await.is_ok() {
            language = Some("Rust".to_string());
            build_system = Some("Cargo".to_string());
        } else if fs::metadata(path.join("package.json")).await.is_ok() {
            language = Some("JavaScript/TypeScript".to_string());
            build_system = Some("npm/yarn/pnpm".to_string());
        } else if fs::metadata(path.join("pyproject.toml")).await.is_ok() || fs::metadata(path.join("requirements.txt")).await.is_ok() {
            language = Some("Python".to_string());
            build_system = Some("pip/poetry/uv".to_string());
        } else if fs::metadata(path.join("go.mod")).await.is_ok() {
            language = Some("Go".to_string());
            build_system = Some("go modules".to_string());
        }
        
        Ok((is_git, language, build_system))
    }

    pub async fn glob_list(&self, pattern: &Path) -> Result<Vec<String>, FsError> {
        let pattern_str = pattern.to_string_lossy().to_string();
        tokio::task::spawn_blocking(move || {
            let mut paths = Vec::new();
            for entry in glob::glob(&pattern_str).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))? {
                if let Ok(path) = entry {
                    paths.push(path.to_string_lossy().to_string());
                }
            }
            Ok(paths)
        }).await.map_err(|e| FsError::Io(std::io::Error::other(e)))?
    }

    async fn copy_inner(&self, src: &Path, dst: &Path) -> Result<(), FsError> {
        if let Some(parent) = dst.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).await?;
            }
        }
        let meta = fs::symlink_metadata(src).await?;
        if meta.is_dir() {
            let src = src.to_path_buf();
            let dst = dst.to_path_buf();
            tokio::task::spawn_blocking(move || -> Result<(), std::io::Error> {
                copy_dir_recursive(&src, &dst)
            })
            .await
            .map_err(|e| FsError::Io(std::io::Error::other(e)))??;
            Ok(())
        } else {
            fs::copy(src, dst).await?;
            Ok(())
        }
    }
}

fn count_recursive(path: &Path) -> Result<usize, std::io::Error> {
    let mut n = 0usize;
    let mut stack = std::collections::VecDeque::from([path.to_path_buf()]);
    while let Some(d) = stack.pop_front() {
        for entry in std::fs::read_dir(&d)? {
            let entry = entry?;
            n += 1;
            let ft = entry.file_type()?;
            if ft.is_dir() {
                stack.push_back(entry.path());
            }
        }
    }
    Ok(n)
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if ft.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else if ft.is_symlink() {
            let target = std::fs::read_link(&from)?;
            #[cfg(unix)]
            std::os::unix::fs::symlink(&target, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use std::io::Write;

    #[tokio::test]
    async fn test_grep() {
        let dir = tempdir().unwrap();
        let fs = FileSystem;
        let file_path = dir.path().join("test.txt");
        let mut file = std::fs::File::create(&file_path).unwrap();
        writeln!(file, "hello world\nthis is a test\nanother line").unwrap();

        let matches = fs.grep(dir.path(), "test").await.unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].line, 2);
        assert_eq!(matches[0].content, "this is a test");
    }

    #[tokio::test]
    async fn test_tree() {
        let dir = tempdir().unwrap();
        let fs = FileSystem;
        let file_path = dir.path().join("test.txt");
        std::fs::File::create(&file_path).unwrap();

        let root = fs.tree(dir.path()).await.unwrap();
        assert_eq!(root.name, ".");
        assert!(root.is_dir);
        let children = root.children.unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, "test.txt");
        assert!(!children[0].is_dir);
    }

    #[tokio::test]
    async fn test_stat() {
        let dir = tempdir().unwrap();
        let fs = FileSystem;
        let file_path = dir.path().join("test.txt");
        let mut file = std::fs::File::create(&file_path).unwrap();
        write!(file, "12345").unwrap();

        let (size, _, is_dir, is_file, is_symlink, _) = fs.stat(&file_path).await.unwrap();
        assert_eq!(size, 5);
        assert!(!is_dir);
        assert!(is_file);
        assert!(!is_symlink);
    }

    #[tokio::test]
    async fn test_project_info() {
        let dir = tempdir().unwrap();
        let fs = FileSystem;
        
        let cargo_toml = dir.path().join("Cargo.toml");
        std::fs::File::create(&cargo_toml).unwrap();

        let (is_git, language, build_system) = fs.project_info(dir.path()).await.unwrap();
        assert!(!is_git);
        assert_eq!(language.as_deref(), Some("Rust"));
        assert_eq!(build_system.as_deref(), Some("Cargo"));
    }

    #[tokio::test]
    async fn test_read_partial() {
        let dir = tempdir().unwrap();
        let fs = FileSystem;
        let file_path = dir.path().join("test_read.txt");
        let mut file = std::fs::File::create(&file_path).unwrap();
        write!(file, "hello world").unwrap();

        let (content, bytes) = fs.read(&file_path, Some(6), Some(5)).await.unwrap();
        assert_eq!(content, "world");
        assert_eq!(bytes, 5);
        
        let (content2, _) = fs.read(&file_path, None, Some(5)).await.unwrap();
        assert_eq!(content2, "hello");
    }

    #[tokio::test]
    async fn test_edit_patch() {
        let dir = tempdir().unwrap();
        let fs = FileSystem;
        let file_path = dir.path().join("test_edit.txt");
        let mut file = std::fs::File::create(&file_path).unwrap();
        write!(file, "foo bar baz").unwrap();

        let (bytes, matches, diff) = fs.edit(
            &file_path,
            Some("bar"),
            Some("qux"),
            Some(false),
            Some(true),
            None,
            None,
            None,
            None,
            Some(true),
        ).await.unwrap();
        
        assert_eq!(matches, 1);
        assert!(diff.is_some());
        assert!(diff.unwrap().contains("-foo bar baz"));
        assert!(diff.unwrap().contains("+foo qux baz"));
        
        // Ensure not modified (dry run)
        let (content, _) = fs.read(&file_path, None, None).await.unwrap();
        assert_eq!(content, "foo bar baz");

        // Real edit
        fs.edit(&file_path, Some("bar"), Some("qux"), None, None, None, None, None, None, None).await.unwrap();
        let (content, _) = fs.read(&file_path, None, None).await.unwrap();
        assert_eq!(content, "foo qux baz");
    }

    #[tokio::test]
    async fn test_glob_list() {
        let dir = tempdir().unwrap();
        let fs = FileSystem;
        let file_path = dir.path().join("test.rs");
        std::fs::File::create(&file_path).unwrap();
        let file_path2 = dir.path().join("test.txt");
        std::fs::File::create(&file_path2).unwrap();

        let glob_pattern = dir.path().join("*.rs");
        let paths = fs.glob_list(&glob_pattern).await.unwrap();
        assert_eq!(paths.len(), 1);
        assert!(paths[0].ends_with("test.rs"));
    }
}
