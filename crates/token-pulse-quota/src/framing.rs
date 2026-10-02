use crate::MAX_PROTOCOL_LINE_BYTES;
use std::io::BufRead;
use token_pulse_core::error::ErrorCode;

/// Read at most one bounded JSONL frame. Never allocate from a remote length hint.
pub(crate) fn read_frame(reader: &mut impl BufRead) -> Result<Option<Vec<u8>>, ErrorCode> {
    let mut line = Vec::new();
    loop {
        let buffer = reader
            .fill_buf()
            .map_err(|_| ErrorCode::QuotaServiceUnavailable)?;
        if buffer.is_empty() {
            return if line.is_empty() {
                Ok(None)
            } else {
                Err(ErrorCode::QuotaProtocolError)
            };
        }
        let newline = buffer.iter().position(|b| *b == b'\n');
        let length = newline.unwrap_or(buffer.len());
        if line.len().saturating_add(length) > MAX_PROTOCOL_LINE_BYTES {
            return Err(ErrorCode::QuotaProtocolError);
        }
        line.extend_from_slice(&buffer[..length]);
        reader.consume(length + usize::from(newline.is_some()));
        if newline.is_some() {
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            return Ok(Some(line));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufReader, Cursor};
    #[test]
    fn bounded_frames_preserve_boundaries_and_reject_truncation() {
        let mut input = BufReader::with_capacity(2, Cursor::new(b"{}\r\n[]\n"));
        assert_eq!(read_frame(&mut input).unwrap(), Some(b"{}".to_vec()));
        assert_eq!(read_frame(&mut input).unwrap(), Some(b"[]".to_vec()));
        assert_eq!(read_frame(&mut input).unwrap(), None);
        assert_eq!(
            read_frame(&mut Cursor::new(b"{}")),
            Err(ErrorCode::QuotaProtocolError)
        );
        let mut oversized = vec![b' '; MAX_PROTOCOL_LINE_BYTES + 1];
        oversized.push(b'\n');
        assert_eq!(
            read_frame(&mut BufReader::with_capacity(17, Cursor::new(oversized))),
            Err(ErrorCode::QuotaProtocolError)
        );
        let mut exact = vec![b' '; MAX_PROTOCOL_LINE_BYTES];
        exact.push(b'\n');
        assert_eq!(
            read_frame(&mut Cursor::new(exact)).unwrap().unwrap().len(),
            MAX_PROTOCOL_LINE_BYTES
        );
    }
}
