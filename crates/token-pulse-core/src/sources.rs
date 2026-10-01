//! Source discovery enumerates only the two rollout trees, never Codex credentials/configuration.
use crate::error::ErrorCode;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, ReadDir},
    path::{Component, Path, PathBuf},
    time::SystemTime,
};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum SourceOrigin {
    WindowsDefault,
    Environment,
    Custom,
    Wsl,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum SourceReadability {
    AwaitingDirectory,
    Readable,
    PartiallyReadable,
    Unreadable,
    Disabled,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityState {
    NotProbed,
    Available,
    Unavailable,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
#[serde(default)]
pub struct SourceCapabilities {
    pub physical_identity: CapabilityState,
    pub byte_seek: CapabilityState,
    pub watcher: CapabilityState,
    pub polling_required: bool,
}
impl Default for SourceCapabilities {
    fn default() -> Self {
        Self {
            physical_identity: CapabilityState::NotProbed,
            byte_seek: CapabilityState::NotProbed,
            watcher: CapabilityState::NotProbed,
            polling_required: true,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct SourceSummary {
    pub source_id: String,
    pub root_path: String,
    pub origin: SourceOrigin,
    pub enabled: bool,
    pub removed: bool,
    pub readability: SourceReadability,
    pub capabilities: SourceCapabilities,
    pub last_scan_at_ms: Option<crate::numeric::EpochMs>,
    pub last_success_at_ms: Option<crate::numeric::EpochMs>,
    pub error: Option<ErrorCode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum SourceDirectoryKind {
    Local,
    Wsl,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct SourceDirectorySelection {
    pub selection_handle: String,
    pub root_path: String,
    pub origin: SourceOrigin,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct SourcesSnapshot {
    pub settings_revision: crate::numeric::DecimalInt,
    pub sources: Vec<SourceSummary>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ManageSourceAction {
    Add { selection_handle: String },
    Pause { source_id: String },
    Resume { source_id: String },
    Detect {},
    RetainRemove { source_id: String },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceCandidate {
    pub root: PathBuf,
    pub origin: SourceOrigin,
}

/// Explicit directories take precedence. Callers supply process-visible values, making tests hermetic.
pub fn local_candidates(
    explicit: &[PathBuf],
    environment_home: Option<PathBuf>,
    user_home: Option<PathBuf>,
) -> Vec<SourceCandidate> {
    let mut result = vec![];
    for path in explicit {
        push_unique(&mut result, path.clone(), SourceOrigin::Custom);
    }
    if let Some(path) = environment_home {
        push_unique(&mut result, path, SourceOrigin::Environment);
    }
    if let Some(path) = user_home {
        push_unique(
            &mut result,
            path.join(".codex"),
            SourceOrigin::WindowsDefault,
        );
    }
    result
}
fn push_unique(result: &mut Vec<SourceCandidate>, root: PathBuf, origin: SourceOrigin) {
    if validate_root(&root, origin).is_err() {
        return;
    }
    let key = directory_key(&root);
    if result.iter().all(|s| directory_key(&s.root) != key) {
        result.push(SourceCandidate { root, origin });
    }
}
pub fn directory_key(path: &Path) -> String {
    let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let key = canonical.to_string_lossy().replace('/', "\\");
    if cfg!(windows) {
        key.to_lowercase()
    } else {
        canonical.to_string_lossy().into_owned()
    }
}
pub fn validate_root(path: &Path, origin: SourceOrigin) -> Result<(), ErrorCode> {
    let text = path.to_str().ok_or(ErrorCode::InvalidQuery)?;
    if text.len() > 32768
        || text.contains('\0')
        || !path.is_absolute()
        || path.components().any(|c| matches!(c, Component::ParentDir))
    {
        return Err(ErrorCode::InvalidQuery);
    }
    let unc = text.starts_with("\\\\") || text.starts_with("//");
    if origin == SourceOrigin::Wsl {
        let normalized = text.replace('/', "\\").to_lowercase();
        if !normalized.starts_with("\\\\wsl.localhost\\") && !normalized.starts_with("\\\\wsl$\\") {
            return Err(ErrorCode::InvalidQuery);
        }
        let parts: Vec<_> = normalized.split('\\').filter(|s| !s.is_empty()).collect();
        if parts.len() < 3 || matches!(parts[1], "." | "..") || parts[1].contains(':') {
            return Err(ErrorCode::InvalidQuery);
        }
    } else if unc {
        let bytes = text.as_bytes();
        if !text.starts_with("\\\\?\\")
            || bytes.len() < 7
            || !bytes[4].is_ascii_alphabetic()
            || bytes[5] != b':'
            || bytes[6] != b'\\'
        {
            return Err(ErrorCode::InvalidQuery);
        }
    }
    Ok(())
}

pub struct DiscoveredFile {
    pub path: PathBuf,
    pub size: u64,
    pub modified_at: Option<SystemTime>,
}
#[derive(Debug)]
pub struct DiscoveryIssue {
    pub path: PathBuf,
    pub readability: SourceReadability,
}
enum Entry {
    Directory(PathBuf),
    Read { path: PathBuf, read: Box<ReadDir> },
}
/// A resumable iterator has no 10,000-file truncation; at most 64 directory handles are active.
pub struct SourceScanner {
    stack: Vec<Entry>,
}
impl SourceScanner {
    pub fn new(root: &Path, origin: SourceOrigin) -> Result<Self, ErrorCode> {
        validate_root(root, origin)?;
        Ok(Self {
            stack: vec![
                Entry::Directory(root.join("archived_sessions")),
                Entry::Directory(root.join("sessions")),
            ],
        })
    }
}
impl Iterator for SourceScanner {
    type Item = Result<DiscoveredFile, DiscoveryIssue>;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.stack.pop()? {
                Entry::Directory(path) => {
                    if fs::symlink_metadata(&path).is_ok_and(|metadata| is_reparse(&metadata)) {
                        return Some(Err(DiscoveryIssue {
                            path,
                            readability: SourceReadability::Unreadable,
                        }));
                    }
                    match fs::read_dir(&path) {
                        Ok(read) => self.stack.push(Entry::Read {
                            path,
                            read: Box::new(read),
                        }),
                        Err(e) => {
                            return Some(Err(DiscoveryIssue {
                                path,
                                readability: if e.kind() == std::io::ErrorKind::NotFound {
                                    SourceReadability::AwaitingDirectory
                                } else {
                                    SourceReadability::Unreadable
                                },
                            }));
                        }
                    }
                }
                Entry::Read {
                    path: directory_path,
                    mut read,
                } => {
                    let Some(entry) = read.next() else {
                        continue;
                    };
                    self.stack.push(Entry::Read {
                        path: directory_path.clone(),
                        read,
                    });
                    let entry = match entry {
                        Ok(entry) => entry,
                        Err(_) => {
                            return Some(Err(DiscoveryIssue {
                                path: directory_path,
                                readability: SourceReadability::Unreadable,
                            }));
                        }
                    };
                    let path = entry.path();
                    let metadata = match fs::symlink_metadata(&path) {
                        Ok(m) => m,
                        Err(_) => {
                            return Some(Err(DiscoveryIssue {
                                path,
                                readability: SourceReadability::Unreadable,
                            }));
                        }
                    };
                    if is_reparse(&metadata) {
                        continue;
                    }
                    if metadata.is_dir() {
                        if self.stack.len() >= 64 {
                            return Some(Err(DiscoveryIssue {
                                path,
                                readability: SourceReadability::Unreadable,
                            }));
                        }
                        self.stack.push(Entry::Directory(path));
                    } else if metadata.is_file()
                        && path
                            .extension()
                            .is_some_and(|s| s.eq_ignore_ascii_case("jsonl"))
                    {
                        return Some(Ok(DiscoveredFile {
                            path,
                            size: metadata.len(),
                            modified_at: metadata.modified().ok(),
                        }));
                    }
                }
            }
        }
    }
}
fn is_reparse(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}
