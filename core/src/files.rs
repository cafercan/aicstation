//! Files of a session folder for the message box's `@` suggestions: a
//! bounded walk (or git's file list) and the ranking of the paths against
//! what is typed after the `@`.
use serde::Serialize;
use std::collections::{BTreeSet, VecDeque};
use std::path::Path;

/// A file or folder under the session folder; `path` is relative, with `/`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHint {
    pub path: String,
    pub dir: bool,
}

/// Most entries kept for one folder.
pub const MAX_ENTRIES: usize = 20_000;
const MAX_DEPTH: usize = 12;
/// Folders never walked into: tool output and caches nobody names with `@`.
const SKIP: [&str; 11] =
    [".git", "node_modules", "target", ".venv", "venv", "__pycache__", ".next", ".cache", ".vs", ".idea", ".gradle"];

/// Files and folders under `root`, breadth first (so a cut keeps the upper
/// levels), at most `max`. Symlinks and junctions are listed but not followed.
pub fn walk(root: &Path, max: usize) -> Vec<FileHint> {
    let mut out = Vec::new();
    let mut queue = VecDeque::from([(root.to_path_buf(), String::new(), 0usize)]);
    while let Some((dir, rel, depth)) = queue.pop_front() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        let mut entries: Vec<(String, bool, bool)> = rd
            .filter_map(Result::ok)
            .filter_map(|e| {
                let name = e.file_name().into_string().ok()?;
                let ft = e.file_type().ok()?;
                Some((name, ft.is_dir(), ft.is_symlink()))
            })
            .collect();
        entries.sort();
        for (name, is_dir, link) in entries {
            if out.len() >= max {
                return out;
            }
            if is_dir && SKIP.contains(&name.as_str()) {
                continue;
            }
            let path = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
            if is_dir && !link && depth + 1 < MAX_DEPTH {
                queue.push_back((dir.join(&name), path.clone(), depth + 1));
            }
            out.push(FileHint { path, dir: is_dir });
        }
    }
    out
}

/// Files of `git ls-files -z` output plus the folders they are in, at most `max`.
pub fn from_git(out: &[u8], max: usize) -> Vec<FileHint> {
    let mut dirs = BTreeSet::new();
    let mut files = Vec::new();
    for raw in out.split(|b| *b == 0) {
        let Ok(p) = std::str::from_utf8(raw) else { continue };
        if p.is_empty() || files.len() >= max {
            continue;
        }
        let mut at = 0;
        while let Some(i) = p[at..].find('/') {
            dirs.insert(p[..at + i].to_string());
            at += i + 1;
        }
        files.push(FileHint { path: p.to_string(), dir: false });
    }
    let mut all: Vec<FileHint> = dirs.into_iter().map(|path| FileHint { path, dir: true }).collect();
    all.extend(files);
    all.truncate(max);
    all
}

fn base(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

/// `q`'s characters appear in `s` in order.
fn subsequence(s: &str, q: &str) -> bool {
    let mut it = s.chars();
    q.chars().all(|c| it.any(|x| x == c))
}

/// The entries matching `query` (case-insensitive, `\` read as `/`), best
/// first: name or path starting with it, name containing it, path containing
/// it, then its letters in order. Shallower and shorter paths first within a
/// rank. An empty query lists the top level.
pub fn rank(all: &[FileHint], query: &str, limit: usize) -> Vec<FileHint> {
    let q = query.replace('\\', "/").to_lowercase();
    let score = |h: &FileHint| -> Option<u8> {
        let p = h.path.to_lowercase();
        let b = base(&p);
        if q.is_empty() {
            return (!p.contains('/')).then_some(0);
        }
        if p.starts_with(&q) || (!q.contains('/') && b.starts_with(&q)) {
            Some(0)
        } else if b.contains(&q) {
            Some(1)
        } else if p.contains(&q) {
            Some(2)
        } else if subsequence(&p, &q) {
            Some(3)
        } else {
            None
        }
    };
    let mut hits: Vec<(u8, usize, &FileHint)> =
        all.iter().filter_map(|h| score(h).map(|s| (s, h.path.matches('/').count(), h))).collect();
    hits.sort_by(|a, b| (a.0, a.1, a.2.path.len(), &a.2.path).cmp(&(b.0, b.1, b.2.path.len(), &b.2.path)));
    hits.into_iter().take(limit).map(|(_, _, h)| h.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(path: &str, dir: bool) -> FileHint {
        FileHint { path: path.into(), dir }
    }

    fn paths(v: &[FileHint]) -> Vec<&str> {
        v.iter().map(|h| h.path.as_str()).collect()
    }

    #[test]
    fn walk_lists_breadth_first_and_skips_tool_folders() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        std::fs::create_dir_all(r.join("src/deep")).unwrap();
        std::fs::create_dir_all(r.join("node_modules/x")).unwrap();
        std::fs::create_dir_all(r.join(".git")).unwrap();
        std::fs::write(r.join("README.md"), "").unwrap();
        std::fs::write(r.join("src/main.rs"), "").unwrap();
        std::fs::write(r.join("src/deep/a.rs"), "").unwrap();
        std::fs::write(r.join(".env.example"), "").unwrap();
        let all = walk(r, 100);
        assert_eq!(paths(&all), [".env.example", "README.md", "src", "src/deep", "src/main.rs", "src/deep/a.rs"]);
        assert!(all[2].dir && !all[1].dir);
        assert_eq!(paths(&walk(r, 3)), [".env.example", "README.md", "src"]);
    }

    #[test]
    fn git_list_adds_the_folders() {
        let all = from_git(b"a/b/c.rs\0top.md\0a/d.rs\0", 100);
        assert_eq!(paths(&all), ["a", "a/b", "a/b/c.rs", "top.md", "a/d.rs"]);
        assert!(all[0].dir && all[1].dir && !all[2].dir);
        assert_eq!(from_git(b"x\0y\0z\0", 2).len(), 2);
    }

    #[test]
    fn rank_puts_name_starts_first() {
        let all = [
            h("docs/readme-old.txt", false),
            h("README.md", false),
            h("src/reader.rs", false),
            h("src", true),
            h("src/thread.rs", false),
            h("ui/src/Reply.tsx", false),
        ];
        assert_eq!(paths(&rank(&all, "rea", 10)), ["README.md", "src/reader.rs", "docs/readme-old.txt", "src/thread.rs"]);
        assert_eq!(paths(&rank(&all, "src/", 10)), ["src/reader.rs", "src/thread.rs", "ui/src/Reply.tsx"]);
        assert_eq!(paths(&rank(&all, r"SRC\Rea", 10)), ["src/reader.rs", "src/thread.rs"]);
        assert_eq!(paths(&rank(&all, "srt", 10)), ["src/thread.rs", "docs/readme-old.txt", "ui/src/Reply.tsx"]);
        assert_eq!(paths(&rank(&all, "", 10)), ["src", "README.md"]);
        assert_eq!(rank(&all, "rea", 2).len(), 2);
        assert!(rank(&all, "zzz", 10).is_empty());
    }
}
