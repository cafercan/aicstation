//! One image of a transcript line, read again from the file on demand.
use super::parse::{image_content, image_parts, walk_images};
use serde_json::Value;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

const MAX_LINE: usize = crate::transcript::MAX_LINE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageError {
    Gone,
    Io,
}

fn valid_b64(d: &str) -> bool {
    d.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'=' | b'\r' | b'\n'))
}

/// Re-reads the line at `offset` and returns `(media_type, base64)` of the
/// image with the given ordinal. Any mismatch means the file changed: `Gone`.
pub fn read_image(path: &Path, offset: u64, ordinal: u32) -> Result<(String, String), ImageError> {
    read_image_limited(path, offset, ordinal, MAX_LINE)
}

/// [`read_image`] with the longest accepted line passed in (tests).
pub(super) fn read_image_limited(path: &Path, offset: u64, ordinal: u32, max_line: usize) -> Result<(String, String), ImageError> {
    let mut f = File::open(path).map_err(|e| match e.kind() {
        io::ErrorKind::NotFound => ImageError::Gone,
        _ => ImageError::Io,
    })?;
    f.seek(SeekFrom::Start(offset)).map_err(|_| ImageError::Gone)?;
    let mut buf = Vec::new();
    // Up to the limit plus the newline, as the transcript reader accepts it.
    BufReader::new(f.take(max_line as u64 + 1))
        .read_until(b'\n', &mut buf)
        .map_err(|_| ImageError::Io)?;
    let line = buf.strip_suffix(b"\n").unwrap_or(&buf);
    if line.len() > max_line {
        return Err(ImageError::Gone);
    }
    let line = std::str::from_utf8(line).map_err(|_| ImageError::Gone)?;
    let v: Value = serde_json::from_str(line.trim_end()).map_err(|_| ImageError::Gone)?;
    let content = image_content(&v).ok_or(ImageError::Gone)?;
    let mut found = None;
    walk_images(content, &mut 0, &mut |n, b| {
        if n == ordinal {
            found = image_parts(b);
        }
    });
    let (mt, data) = found.ok_or(ImageError::Gone)?;
    if !valid_b64(data) {
        return Err(ImageError::Gone);
    }
    Ok((mt.to_string(), data.to_string()))
}
