//! Bounded reads of the small JSON files Claude Code keeps next to its
//! transcripts (session registry, agent meta).
use std::io::Read;
use std::path::Path;

/// Largest small JSON file that is read at all.
pub const MAX_SMALL_FILE: u64 = 1024 * 1024;

/// Windows `FILE_ATTRIBUTE_REPARSE_POINT` (symlink, junction, cloud stub...).
#[cfg(windows)]
const REPARSE_POINT: u32 = 0x400;

/// Whether `md` (from `symlink_metadata`) is a plain regular file.
fn is_plain_file(md: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if md.file_attributes() & REPARSE_POINT != 0 {
            return false;
        }
    }
    md.file_type().is_file()
}

/// Contents of `path` when it is a regular file (not a symlink, junction or
/// other reparse point) of at most [`MAX_SMALL_FILE`] bytes. A link is never
/// followed, so a linked `x.json` cannot pull in another file (e.g. a `.key`).
pub fn read_small_file(path: &Path) -> Option<String> {
    let md = std::fs::symlink_metadata(path).ok()?;
    if !is_plain_file(&md) || md.len() > MAX_SMALL_FILE {
        return None;
    }
    // The file may grow between the check and the read: read one byte past
    // the cap and refuse if it is there.
    let mut buf = Vec::new();
    std::fs::File::open(path).ok()?.take(MAX_SMALL_FILE + 1).read_to_end(&mut buf).ok()?;
    if buf.len() as u64 > MAX_SMALL_FILE {
        return None;
    }
    String::from_utf8(buf).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn reads_small_regular_file_only() {
        let d = tempfile::tempdir().unwrap();
        let ok = d.path().join("a.json");
        fs::write(&ok, "{}").unwrap();
        assert_eq!(read_small_file(&ok).as_deref(), Some("{}"));
        let big = d.path().join("b.json");
        fs::write(&big, vec![b' '; MAX_SMALL_FILE as usize + 1]).unwrap();
        assert_eq!(read_small_file(&big), None);
        let exact = d.path().join("c.json");
        fs::write(&exact, vec![b' '; MAX_SMALL_FILE as usize]).unwrap();
        assert!(read_small_file(&exact).is_some());
        fs::create_dir(d.path().join("dir.json")).unwrap();
        assert_eq!(read_small_file(&d.path().join("dir.json")), None);
        assert_eq!(read_small_file(&d.path().join("missing.json")), None);
    }
}
