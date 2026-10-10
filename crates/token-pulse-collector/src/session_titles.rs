//! Incremental bounded title metadata reads with no file-size ceiling.
use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    path::Path,
    time::SystemTime,
};
use token_pulse_core::{error::ErrorCode, reader::file_identity};
use token_pulse_store::{Database, session_titles::SessionTitle};
const READ_BUDGET: u64 = 4 * 1024 * 1024;
const MAX_LINE_BYTES: u64 = 8 * 1024 * 1024;
const BATCH_TITLES: usize = 500;
type Stamp = (u64, Option<SystemTime>);
#[derive(Default)]
struct Scan {
    stamp: Option<Stamp>,
    identity: String,
    offset: u64,
    discarding: bool,
    invalid_offset: Option<u64>,
    prefix: Vec<u8>,
    tail: Vec<u8>,
}
#[derive(Default)]
pub(crate) struct TitleIndexes {
    scans: BTreeMap<String, (String, Scan)>,
}
impl TitleIndexes {
    pub(crate) fn refresh(&mut self, database: &Database) {
        let Ok(sources) = database.enabled_sources() else {
            return;
        };
        self.scans
            .retain(|id, _| sources.iter().any(|s| &s.source_id == id));
        for source in sources {
            let path = Path::new(&source.root_path).join("session_index.jsonl");
            let previous = self
                .scans
                .remove(&source.source_id)
                .filter(|(root, _)| root == &source.root_path)
                .map(|(_, scan)| scan)
                .unwrap_or_default();
            let at = super::jobs::now_ms().unwrap_or(0);
            match read_chunk(&path, previous) {
                Ok((scan, titles)) => {
                    if titles.is_empty()
                        || database
                            .sync_session_titles(
                                source.source_id.clone(),
                                source.root_path.clone(),
                                titles,
                            )
                            .is_ok()
                    {
                        let issue = scan
                            .invalid_offset
                            .map(|offset| (ErrorCode::TitleIndexInvalid, Some(offset)));
                        let _ = database.record_title_index_issue(
                            source.source_id.clone(),
                            source.root_path.clone(),
                            issue,
                            at,
                        );
                        self.scans
                            .insert(source.source_id, (source.root_path, scan));
                    }
                }
                Err(error) => {
                    if error == ErrorCode::CheckpointConflict {
                        continue;
                    }
                    // An absent optional index is normal. Unreadable files are visible in diagnostics.
                    let missing = fs::symlink_metadata(&path)
                        .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound);
                    let issue = (!missing).then_some((ErrorCode::TitleIndexUnreadable, None));
                    let _ = database.record_title_index_issue(
                        source.source_id,
                        source.root_path,
                        issue,
                        at,
                    );
                }
            }
        }
    }
}
fn index_stamp(path: &Path) -> Result<Stamp, ErrorCode> {
    let metadata = fs::symlink_metadata(path).map_err(|_| ErrorCode::SourceUnreadable)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(ErrorCode::SourceUnreadable);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(ErrorCode::SourceUnreadable);
        }
    }
    Ok((metadata.len(), metadata.modified().ok()))
}
fn bytes_at(file: &mut fs::File, start: u64, size: usize) -> Result<Vec<u8>, ErrorCode> {
    file.seek(SeekFrom::Start(start))
        .map_err(|_| ErrorCode::SourceUnreadable)?;
    let mut bytes = vec![0; size];
    file.read_exact(&mut bytes)
        .map_err(|_| ErrorCode::SourceUnreadable)?;
    Ok(bytes)
}
fn read_chunk(path: &Path, mut scan: Scan) -> Result<(Scan, Vec<SessionTitle>), ErrorCode> {
    let before = index_stamp(path)?;
    let mut file = fs::File::open(path).map_err(|_| ErrorCode::SourceUnreadable)?;
    let identity = file_identity(
        &file,
        &file.metadata().map_err(|_| ErrorCode::SourceUnreadable)?,
    )
    .map_err(|_| ErrorCode::SourceUnreadable)?;
    let reusable = scan.identity == identity
        && before.0 >= scan.offset
        && scan
            .stamp
            .is_none_or(|old| before.0 > old.0 || before == old)
        && bytes_at(&mut file, 0, scan.prefix.len())? == scan.prefix
        && bytes_at(
            &mut file,
            scan.offset.saturating_sub(scan.tail.len() as u64),
            scan.tail.len(),
        )? == scan.tail;
    if !reusable {
        scan = Scan::default();
    }
    if reusable && scan.stamp == Some(before) && scan.offset == before.0 {
        return Ok((scan, vec![]));
    }
    file.seek(SeekFrom::Start(scan.offset))
        .map_err(|_| ErrorCode::SourceUnreadable)?;
    // A captured length prevents a concurrent append writer extending this work indefinitely.
    let mut reader = BufReader::new(file.take(before.0 - scan.offset));
    let mut titles = Vec::new();
    let mut consumed = 0;
    while consumed < READ_BUDGET && titles.len() < BATCH_TITLES {
        let start = scan.offset;
        let cap = if scan.discarding {
            64 * 1024
        } else {
            MAX_LINE_BYTES + 1
        };
        let mut line = Vec::new();
        let size = reader
            .by_ref()
            .take(cap)
            .read_until(b'\n', &mut line)
            .map_err(|_| ErrorCode::SourceUnreadable)?;
        if size == 0 {
            break;
        }
        consumed += size as u64;
        if scan.discarding || size as u64 > MAX_LINE_BYTES {
            scan.invalid_offset.get_or_insert(start);
            scan.offset += size as u64;
            scan.discarding = line.last() != Some(&b'\n');
            continue;
        }
        // Retry an unfinished append from its beginning; it is not a malformed record.
        if line.last() != Some(&b'\n') {
            break;
        }
        scan.offset += size as u64;
        match parse_title(&line) {
            Some(title) => titles.push(title),
            None => {
                scan.invalid_offset.get_or_insert(start);
            }
        }
    }
    let mut file = reader.into_inner().into_inner();
    let current = fs::File::open(path).map_err(|_| ErrorCode::SourceUnreadable)?;
    let current_identity = file_identity(
        &current,
        &current
            .metadata()
            .map_err(|_| ErrorCode::SourceUnreadable)?,
    )
    .map_err(|_| ErrorCode::SourceUnreadable)?;
    // Replaced or concurrently modified chunks are retried, never committed under old identity.
    if current_identity != identity || index_stamp(path)? != before {
        return Err(ErrorCode::CheckpointConflict);
    }
    scan.prefix = bytes_at(&mut file, 0, scan.offset.min(4096) as usize)?;
    let tail_size = scan.offset.min(4096) as usize;
    scan.tail = bytes_at(&mut file, scan.offset - tail_size as u64, tail_size)?;
    scan.identity = identity;
    scan.stamp = Some(before);
    Ok((scan, titles))
}
fn parse_title(line: &[u8]) -> Option<SessionTitle> {
    let value: serde_json::Value = serde_json::from_slice(line).ok()?;
    let updated = chrono::DateTime::parse_from_rfc3339(value["updated_at"].as_str()?).ok()?;
    let title = SessionTitle {
        provider_session_id: value["id"].as_str()?.into(),
        title: value["thread_name"].as_str()?.trim().into(),
        updated_at_ms: updated.timestamp_millis(),
    };
    title.validate().ok()?;
    Some(title)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    fn entry(id: &str, title: &str) -> String {
        serde_json::json!({"id":id,"thread_name":title,"updated_at":"2026-10-07T12:00:00Z"})
            .to_string()
            + "\n"
    }
    #[test]
    fn incremental_append_partial_tail_and_rewrite() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session_index.jsonl");
        let complete = entry("a", "中文标题");
        let partial = entry("b", "second");
        fs::write(&path, format!("{complete}{}", &partial[..20])).unwrap();
        let (scan, titles) = read_chunk(&path, Scan::default()).unwrap();
        assert_eq!(titles[0].title, "中文标题");
        assert_eq!(scan.offset, complete.len() as u64);
        assert!(scan.invalid_offset.is_none());
        fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(&partial.as_bytes()[20..])
            .unwrap();
        let (scan, titles) = read_chunk(&path, scan).unwrap();
        assert_eq!(titles.len(), 1);
        assert_eq!(titles[0].provider_session_id, "b");
        let (scan, titles) = read_chunk(&path, scan).unwrap();
        assert!(titles.is_empty());
        fs::write(&path, format!("{}{}", entry("a", "改名标题"), partial)).unwrap();
        let (_, titles) = read_chunk(&path, scan).unwrap();
        assert_eq!(titles[0].title, "改名标题");
    }
    #[test]
    fn large_index_and_oversized_bad_line_do_not_hide_later_titles() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session_index.jsonl");
        let mut file = fs::File::create(&path).unwrap();
        file.set_len(65 * 1024 * 1024).unwrap();
        file.seek(SeekFrom::End(0)).unwrap();
        file.write_all(format!("\n{}", entry("last", "after bad line")).as_bytes())
            .unwrap();
        drop(file);
        let mut scan = Scan::default();
        let mut found = vec![];
        for _ in 0..30 {
            let (next, titles) = read_chunk(&path, scan).unwrap();
            found.extend(titles);
            let done = next.offset == fs::metadata(&path).unwrap().len();
            scan = next;
            if done {
                break;
            }
        }
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].provider_session_id, "last");
        assert_eq!(scan.invalid_offset, Some(0));
    }
    #[test]
    fn long_additive_metadata_and_batches_preserve_valid_titles() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session_index.jsonl");
        let value = serde_json::json!({"id":"large","thread_name":"title","updated_at":"2026-10-07T12:00:00Z","metadata":"x".repeat(80*1024)});
        let mut data = format!("{value}\n");
        for n in 0..600 {
            data.push_str(&entry(&format!("session-{n}"), "title"));
        }
        fs::write(&path, data).unwrap();
        let (scan, first) = read_chunk(&path, Scan::default()).unwrap();
        assert_eq!(first.len(), BATCH_TITLES);
        assert!(scan.invalid_offset.is_none());
        let (_, second) = read_chunk(&path, scan).unwrap();
        assert_eq!(second.len(), 101);
    }
}
