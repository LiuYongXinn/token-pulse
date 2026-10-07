//! Read only the optional title index in an enabled Codex Home.
use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader, Read},
    path::Path,
    time::SystemTime,
};
use token_pulse_core::error::ErrorCode;
use token_pulse_store::{Database, session_titles::SessionTitle};

const MAX_INDEX_BYTES: u64 = 64 * 1024 * 1024;
const MAX_LINE_BYTES: u64 = 64 * 1024;
type Stamp = (u64, Option<SystemTime>);

#[derive(Default)]
pub(crate) struct TitleIndexes {
    stamps: BTreeMap<String, (String, Stamp)>,
}
impl TitleIndexes {
    pub(crate) fn refresh(&mut self, database: &Database) {
        let Ok(sources) = database.enabled_sources() else {
            return;
        };
        self.stamps
            .retain(|id, _| sources.iter().any(|s| &s.source_id == id));
        for source in sources {
            let path = Path::new(&source.root_path).join("session_index.jsonl");
            let Ok(before) = index_stamp(&path) else {
                continue;
            };
            if before.1.is_some()
                && self.stamps.get(&source.source_id) == Some(&(source.root_path.clone(), before))
            {
                continue;
            }
            let Ok(titles) = read_index(&path) else {
                continue;
            };
            if index_stamp(&path).ok() != Some(before) {
                continue;
            }
            if database
                .sync_session_titles(source.source_id.clone(), source.root_path.clone(), titles)
                .is_ok()
            {
                self.stamps
                    .insert(source.source_id, (source.root_path, before));
            }
        }
    }
}
fn index_stamp(path: &Path) -> Result<Stamp, ErrorCode> {
    let metadata = fs::symlink_metadata(path).map_err(|_| ErrorCode::SourceUnreadable)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > MAX_INDEX_BYTES
    {
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
fn read_index(path: &Path) -> Result<Vec<SessionTitle>, ErrorCode> {
    index_stamp(path)?;
    let file = fs::File::open(path).map_err(|_| ErrorCode::SourceUnreadable)?;
    let mut reader = BufReader::new(file.take(MAX_INDEX_BYTES + 1));
    let mut titles = BTreeMap::<String, SessionTitle>::new();
    let mut consumed = 0;
    loop {
        let mut line = Vec::new();
        let size = reader
            .by_ref()
            .take(MAX_LINE_BYTES + 1)
            .read_until(b'\n', &mut line)
            .map_err(|_| ErrorCode::SourceUnreadable)?;
        if size == 0 {
            break;
        }
        consumed += size as u64;
        if size as u64 > MAX_LINE_BYTES || consumed > MAX_INDEX_BYTES {
            return Err(ErrorCode::SourceUnreadable);
        }
        // Ignore incomplete appends, malformed entries and unknown fields.
        if line.last() != Some(&b'\n') {
            break;
        }
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&line) else {
            continue;
        };
        let (Some(id), Some(name), Some(updated)) = (
            value["id"].as_str(),
            value["thread_name"].as_str(),
            value["updated_at"].as_str(),
        ) else {
            continue;
        };
        let Ok(updated) = chrono::DateTime::parse_from_rfc3339(updated) else {
            continue;
        };
        let title = SessionTitle {
            provider_session_id: id.into(),
            title: name.trim().into(),
            updated_at_ms: updated.timestamp_millis(),
        };
        if title.validate().is_err() {
            continue;
        }
        if titles
            .get(id)
            .is_none_or(|previous| title.updated_at_ms >= previous.updated_at_ms)
        {
            titles.insert(id.into(), title);
        }
    }
    Ok(titles.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn newest_valid_title_wins_and_incomplete_append_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session_index.jsonl");
        fs::write(&path, concat!(
            "{\"id\":\"a\",\"thread_name\":\" 最新中文标题 \",\"updated_at\":\"2026-10-07T12:00:00Z\",\"extra\":1}\r\n",
            "{\"id\":\"a\",\"thread_name\":\"old\",\"updated_at\":\"2026-10-06T12:00:00Z\"}\n",
            "bad json\n",
            "{\"id\":\"b\",\"thread_name\":\" \",\"updated_at\":\"2026-10-07T12:00:00Z\"}\n",
            "{\"id\":\"a\",\"thread_name\":\"partial\",\"updated_at\":\"2026-10-08T12:00:00Z\"}"
        )).unwrap();
        let titles = read_index(&path).unwrap();
        assert_eq!(titles.len(), 1);
        assert_eq!(titles[0].title, "最新中文标题");
        fs::write(&path, vec![b'x'; MAX_LINE_BYTES as usize + 1]).unwrap();
        assert_eq!(read_index(&path).unwrap_err(), ErrorCode::SourceUnreadable);
    }
}
