//! Read-only, bounded JSONL framing. Record bytes are transient and never logged.
use crate::{
    domain::{ContentAnchor, OversizedLineState, PhysicalPosition},
    error::ErrorCode,
};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, Metadata, OpenOptions},
    io::{Read, Seek, SeekFrom},
    path::Path,
    time::SystemTime,
};

pub struct ReaderLimits {
    pub block_bytes: usize,
    pub line_bytes: usize,
    pub batch_bytes: usize,
    pub records: usize,
}
impl Default for ReaderLimits {
    fn default() -> Self {
        Self {
            block_bytes: 64 * 1024,
            line_bytes: 8 * 1024 * 1024,
            batch_bytes: 16 * 1024 * 1024,
            records: 500,
        }
    }
}
#[derive(Default)]
pub struct ReaderCheckpoint {
    pub file_identity: Option<String>,
    pub observed_size: Option<u64>,
    pub committed_offset: u64,
    pub anchors: Vec<ContentAnchor>,
    pub oversized_line: Option<OversizedLineState>,
    pub modified_at: Option<SystemTime>,
}
pub enum FramedLine {
    Complete {
        position: PhysicalPosition,
        bytes: Vec<u8>,
    },
    Oversized {
        position: PhysicalPosition,
    },
}
pub struct ReadBatch {
    pub file_identity: String,
    pub observed_size: u64,
    pub modified_at: Option<SystemTime>,
    pub next_offset: u64,
    pub anchors: Vec<ContentAnchor>,
    pub lines: Vec<FramedLine>,
    pub oversized_line: Option<OversizedLineState>,
    pub has_more: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadError {
    Io,
    InvalidPath,
    InvalidLimits,
    InvalidGeneration,
    NumericOverflow,
}
impl ReadError {
    pub fn code(self) -> ErrorCode {
        match self {
            Self::Io => ErrorCode::SourceUnreadable,
            Self::InvalidGeneration => ErrorCode::CheckpointConflict,
            Self::NumericOverflow => ErrorCode::NumericOverflow,
            _ => ErrorCode::InvalidQuery,
        }
    }
}

/// Re-read at most 64 saved diagnostic positions, with a 64 KiB limit per record.
/// Covering anchors are verified before and after the read; a rewrite cannot
/// turn an older diagnostic into a successful classification of new bytes.
pub fn read_saved_diagnostic_lines(
    path: &Path,
    checkpoint: &ReaderCheckpoint,
    offsets: &[u64],
) -> Result<Vec<Option<Vec<u8>>>, ReadError> {
    const RECORD_BYTES: u64 = 64 * 1024;
    if offsets.len() > 64
        || offsets.iter().any(|&o| o >= checkpoint.committed_offset)
        || checkpoint.file_identity.is_none()
        || !path
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("jsonl"))
    {
        return Err(ReadError::InvalidLimits);
    }
    let mut file = open_read_only(path)?;
    let metadata = file.metadata().map_err(|_| ReadError::Io)?;
    if checkpoint.file_identity.as_ref() != Some(&file_identity(&file, &metadata)?)
        || metadata.len() < checkpoint.committed_offset
        || checkpoint.observed_size.is_some_and(|n| metadata.len() < n)
    {
        return Err(ReadError::InvalidGeneration);
    }
    let mut selected = std::collections::BTreeSet::new();
    for &offset in offsets {
        let end = offset
            .saturating_add(RECORD_BYTES + 1)
            .min(checkpoint.committed_offset);
        let mut covered = offset;
        for (index, anchor) in checkpoint.anchors.iter().enumerate() {
            let anchor_end = anchor
                .byte_offset
                .checked_add(u64::from(anchor.byte_length))
                .ok_or(ReadError::InvalidGeneration)?;
            if anchor_end > checkpoint.committed_offset {
                return Err(ReadError::InvalidGeneration);
            }
            if anchor.byte_offset < end && anchor_end > offset {
                selected.insert(index);
                if anchor.byte_offset <= covered {
                    covered = covered.max(anchor_end);
                }
            }
        }
        if covered < end {
            return Err(ReadError::InvalidGeneration);
        }
    }
    let verify = |file: &mut File| -> Result<(), ReadError> {
        for &index in &selected {
            let a = &checkpoint.anchors[index];
            if hash_range(file, a.byte_offset, u64::from(a.byte_length))? != a.sha256 {
                return Err(ReadError::InvalidGeneration);
            }
        }
        Ok(())
    };
    verify(&mut file)?;
    let mut lines = Vec::with_capacity(offsets.len());
    for &offset in offsets {
        if offset > 0 {
            file.seek(SeekFrom::Start(offset - 1))
                .map_err(|_| ReadError::Io)?;
            let mut previous = [0];
            file.read_exact(&mut previous).map_err(|_| ReadError::Io)?;
            if previous[0] != b'\n' {
                return Err(ReadError::InvalidGeneration);
            }
        }
        file.seek(SeekFrom::Start(offset))
            .map_err(|_| ReadError::Io)?;
        let mut bytes = Vec::new();
        (&mut file)
            .take((checkpoint.committed_offset - offset).min(RECORD_BYTES + 1))
            .read_to_end(&mut bytes)
            .map_err(|_| ReadError::Io)?;
        lines.push(
            bytes
                .iter()
                .position(|&b| b == b'\n')
                .filter(|&n| n <= RECORD_BYTES as usize)
                .map(|n| {
                    bytes.truncate(n);
                    if bytes.last() == Some(&b'\r') {
                        bytes.pop();
                    }
                    bytes
                }),
        );
    }
    verify(&mut file)?;
    Ok(lines)
}

pub fn read_batch(
    path: &Path,
    generation: &str,
    checkpoint: &ReaderCheckpoint,
    limits: &ReaderLimits,
) -> Result<ReadBatch, ReadError> {
    if !path
        .extension()
        .is_some_and(|s| s.eq_ignore_ascii_case("jsonl"))
    {
        return Err(ReadError::InvalidPath);
    }
    if generation.is_empty()
        || limits.block_bytes == 0
        || limits.block_bytes > 64 * 1024
        || limits.line_bytes == 0
        || limits.records == 0
        || limits.batch_bytes < limits.line_bytes.saturating_add(limits.block_bytes)
        || limits.line_bytes > 8 * 1024 * 1024
        || limits.batch_bytes > 16 * 1024 * 1024
        || limits.records > 500
    {
        return Err(ReadError::InvalidLimits);
    }
    let mut file = open_read_only(path)?;
    let metadata = file.metadata().map_err(|_| ReadError::Io)?;
    let identity = file_identity(&file, &metadata)?;
    let upper = metadata.len();
    let modified_at = metadata.modified().ok();
    if upper > i64::MAX as u64
        || checkpoint.committed_offset > upper
        || checkpoint.observed_size.is_some_and(|size| upper < size)
        || checkpoint
            .file_identity
            .as_ref()
            .is_some_and(|id| id != &identity)
    {
        return Err(ReadError::InvalidGeneration);
    }
    for anchor in &checkpoint.anchors {
        if anchor
            .byte_offset
            .checked_add(u64::from(anchor.byte_length))
            .is_none_or(|end| end > checkpoint.committed_offset)
            || ((checkpoint.modified_at.is_none()
                || checkpoint.modified_at != modified_at
                || anchor.byte_length <= 4096)
                && hash_range(&mut file, anchor.byte_offset, u64::from(anchor.byte_length))?
                    != anchor.sha256)
        {
            return Err(ReadError::InvalidGeneration);
        }
    }
    let mut skipping = checkpoint.oversized_line.clone();
    let mut start = checkpoint.committed_offset;
    let from = if let Some(skip) = &skipping {
        if skip.start_offset != start || skip.scan_offset < start || skip.scan_offset > upper {
            return Err(ReadError::InvalidGeneration);
        }
        for anchor in &skip.anchors {
            if anchor.byte_offset < start
                || anchor
                    .byte_offset
                    .checked_add(u64::from(anchor.byte_length))
                    .is_none_or(|end| end > skip.scan_offset)
                || ((checkpoint.modified_at.is_none()
                    || checkpoint.modified_at != modified_at
                    || anchor.byte_length <= 4096)
                    && hash_range(&mut file, anchor.byte_offset, u64::from(anchor.byte_length))?
                        != anchor.sha256)
            {
                return Err(ReadError::InvalidGeneration);
            }
        }
        skip.scan_offset
    } else {
        start
    };
    file.seek(SeekFrom::Start(from))
        .map_err(|_| ReadError::Io)?;
    let mut buffer = vec![0u8; limits.block_bytes];
    let mut line = Vec::new();
    let mut lines = Vec::new();
    let mut position = from;
    let mut next = start;
    let mut scanned = 0usize;
    let mut digest = Sha256::new();
    while position < upper && scanned < limits.batch_bytes && lines.len() < limits.records {
        let count = (upper - position).min((limits.batch_bytes - scanned).min(buffer.len()) as u64)
            as usize;
        let read = file.read(&mut buffer[..count]).map_err(|_| ReadError::Io)?;
        if read == 0 {
            return Err(ReadError::InvalidGeneration);
        }
        let mut consumed = 0;
        for &byte in &buffer[..read] {
            consumed += 1;
            position += 1;
            scanned += 1;
            if byte == b'\n' {
                let physical = PhysicalPosition {
                    file_generation_id: generation.into(),
                    byte_offset: start,
                    byte_end: position,
                };
                if skipping.take().is_some() {
                    lines.push(FramedLine::Oversized { position: physical });
                } else {
                    if line.last() == Some(&b'\r') {
                        line.pop();
                    }
                    lines.push(FramedLine::Complete {
                        position: physical,
                        bytes: std::mem::take(&mut line),
                    });
                }
                next = position;
                start = position;
                if lines.len() == limits.records {
                    break;
                }
            } else if skipping.is_none() {
                if line.len() == limits.line_bytes {
                    line.clear();
                    skipping = Some(OversizedLineState {
                        start_offset: start,
                        scan_offset: position,
                        anchors: vec![],
                    });
                } else {
                    line.push(byte);
                }
            }
        }
        digest.update(&buffer[..consumed]);
    }
    if let Some(skip) = &mut skipping {
        skip.scan_offset = position;
        skip.anchors = range_anchors(&mut file, skip.start_offset, position, &skip.anchors)?;
    }
    // Validate all bytes actually consumed. Appending beyond the fixed upper bound is allowed.
    let consumed_hash = format!("{:x}", digest.finalize());
    let after = file.metadata().map_err(|_| ReadError::Io)?;
    if after.len() < upper
        || file_identity(&file, &after)? != identity
        || hash_range(&mut file, from, position - from)? != consumed_hash
    {
        return Err(ReadError::InvalidGeneration);
    }
    if after.modified().ok() != modified_at {
        for anchor in checkpoint
            .anchors
            .iter()
            .chain(checkpoint.oversized_line.iter().flat_map(|s| &s.anchors))
        {
            if hash_range(&mut file, anchor.byte_offset, u64::from(anchor.byte_length))?
                != anchor.sha256
            {
                return Err(ReadError::InvalidGeneration);
            }
        }
    }
    // A renamed/replaced path may leave our original handle valid; do not publish it as the new path.
    let current = open_read_only(path).map_err(|_| ReadError::InvalidGeneration)?;
    if file_identity(&current, &current.metadata().map_err(|_| ReadError::Io)?)? != identity {
        return Err(ReadError::InvalidGeneration);
    }
    let anchors = anchors(&mut file, next, &checkpoint.anchors)?;
    Ok(ReadBatch {
        file_identity: identity,
        observed_size: upper,
        modified_at,
        next_offset: next,
        anchors,
        lines,
        oversized_line: skipping,
        has_more: position < upper,
    })
}

fn open_read_only(path: &Path) -> Result<File, ReadError> {
    if std::fs::symlink_metadata(path)
        .map_err(|_| ReadError::Io)?
        .file_type()
        .is_symlink()
    {
        return Err(ReadError::InvalidPath);
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path).map_err(|_| ReadError::Io)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if file
            .metadata()
            .map_err(|_| ReadError::Io)?
            .file_attributes()
            & 0x400
            != 0
        {
            return Err(ReadError::InvalidPath);
        }
    }
    Ok(file)
}

fn hash_range(file: &mut File, offset: u64, length: u64) -> Result<String, ReadError> {
    file.seek(SeekFrom::Start(offset))
        .map_err(|_| ReadError::Io)?;
    let mut remaining = length;
    let mut buffer = [0u8; 64 * 1024];
    let mut hash = Sha256::new();
    while remaining > 0 {
        let limit = remaining.min(buffer.len() as u64) as usize;
        let n = file.read(&mut buffer[..limit]).map_err(|_| ReadError::Io)?;
        if n == 0 {
            return Err(ReadError::InvalidGeneration);
        }
        hash.update(&buffer[..n]);
        remaining -= n as u64;
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn anchors(
    file: &mut File,
    end: u64,
    previous: &[ContentAnchor],
) -> Result<Vec<ContentAnchor>, ReadError> {
    range_anchors(file, 0, end, previous)
}
fn range_anchors(
    file: &mut File,
    start: u64,
    end: u64,
    previous: &[ContentAnchor],
) -> Result<Vec<ContentAnchor>, ReadError> {
    if end == start {
        return Ok(vec![]);
    }
    const SEGMENT: u64 = 8 * 1024 * 1024;
    let mut ranges = vec![
        (start, (end - start).min(4096) as u32),
        (
            end.saturating_sub(4096).max(start),
            (end - start).min(4096) as u32,
        ),
    ];
    let mut offset = start;
    while offset < end {
        if ranges.len() >= 4096 {
            return Err(ReadError::InvalidLimits);
        }
        ranges.push((offset, (end - offset).min(SEGMENT) as u32));
        offset += SEGMENT;
    }
    ranges.sort_unstable();
    ranges.dedup();
    ranges
        .into_iter()
        .map(|(offset, length)| {
            let hash = previous
                .iter()
                .find(|a| a.byte_offset == offset && a.byte_length == length)
                .map(|a| a.sha256.clone());
            Ok(ContentAnchor {
                byte_offset: offset,
                byte_length: length,
                sha256: match hash {
                    Some(h) => h,
                    None => hash_range(file, offset, u64::from(length))?,
                },
            })
        })
        .collect()
}
#[cfg(windows)]
pub fn file_identity(file: &File, _: &Metadata) -> Result<String, ReadError> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };
    let mut information = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
    // The read-only File owns a valid handle; Win32 writes the initialized structure on success.
    let result =
        unsafe { GetFileInformationByHandle(file.as_raw_handle(), information.as_mut_ptr()) };
    if result == 0 {
        return Err(ReadError::Io);
    }
    let information = unsafe { information.assume_init() };
    Ok(format!(
        "win:{}:{}:{}:{}:{}",
        information.dwVolumeSerialNumber,
        information.nFileIndexHigh,
        information.nFileIndexLow,
        information.ftCreationTime.dwHighDateTime,
        information.ftCreationTime.dwLowDateTime
    ))
}
#[cfg(unix)]
pub fn file_identity(_: &File, metadata: &Metadata) -> Result<String, ReadError> {
    use std::os::unix::fs::MetadataExt;
    Ok(format!("unix:{}:{}", metadata.dev(), metadata.ino()))
}
