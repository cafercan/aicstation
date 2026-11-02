//! Locates transcript files and subagent folders for a session id.
use std::collections::HashMap;
use std::path::PathBuf;

pub struct Locator {
    projects_dir: PathBuf,
    cache: HashMap<String, PathBuf>,
}

impl Locator {
    pub fn new(projects_dir: PathBuf) -> Self {
        Self { projects_dir, cache: HashMap::new() }
    }

    /// `<proj>/<id>.jsonl`, searched in every project folder (folder names are lossy).
    pub fn transcript(&mut self, session_id: &str) -> Option<PathBuf> {
        if !is_session_id(session_id) {
            return None;
        }
        if let Some(p) = self.cache.get(session_id) {
            if p.is_file() {
                return Some(p.clone());
            }
            self.cache.remove(session_id);
        }
        let file = format!("{session_id}.jsonl");
        for entry in std::fs::read_dir(&self.projects_dir).ok()?.flatten() {
            let cand = entry.path().join(&file);
            if cand.is_file() {
                self.cache.insert(session_id.to_string(), cand.clone());
                return Some(cand);
            }
        }
        None
    }

    /// `<proj>/<id>/subagents`, if it exists.
    pub fn subagents_dir(&mut self, session_id: &str) -> Option<PathBuf> {
        let t = self.transcript(session_id)?;
        let dir = t.parent()?.join(session_id).join("subagents");
        dir.is_dir().then_some(dir)
    }

    /// Top-level `<proj>/*.jsonl` files only (no subagent transcripts).
    pub fn all_transcripts(&self) -> Vec<(PathBuf, std::fs::Metadata)> {
        let mut out = Vec::new();
        let Ok(projects) = std::fs::read_dir(&self.projects_dir) else { return out };
        for proj in projects.flatten() {
            let Ok(files) = std::fs::read_dir(proj.path()) else { continue };
            for f in files.flatten() {
                let p = f.path();
                if p.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                    continue;
                }
                if let Ok(md) = f.metadata() {
                    if md.is_file() {
                        out.push((p, md));
                    }
                }
            }
        }
        out
    }
}

/// Lowercase UUID: 8-4-4-4-12 hex. Blocks path traversal.
pub fn is_session_id(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => *c == b'-',
            _ => matches!(c, b'0'..=b'9' | b'a'..=b'f'),
        })
}

/// `CLAUDE_CONFIG_DIR` if set and non-empty, else `<home>/.claude`.
pub fn claude_dir() -> PathBuf {
    if let Some(v) = std::env::var_os("CLAUDE_CONFIG_DIR") {
        if !v.is_empty() {
            return PathBuf::from(v);
        }
    }
    let home = std::env::var_os("USERPROFILE")
        .filter(|v| !v.is_empty())
        .or_else(|| std::env::var_os("HOME").filter(|v| !v.is_empty()))
        .map(PathBuf::from)
        .unwrap_or_default();
    home.join(".claude")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());
    const ID_A: &str = "aaaaaaaa-1111-4222-8333-444444444444";
    const ID_B: &str = "bbbbbbbb-1111-4222-8333-444444444444";

    fn write(p: &std::path::Path, s: &str) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, s).unwrap();
    }

    #[test]
    fn finds_transcript_in_any_project_dir() {
        let t = tempfile::tempdir().unwrap();
        write(&t.path().join("d--x").join(format!("{ID_A}.jsonl")), "{}\n");
        write(&t.path().join("D--y").join(format!("{ID_B}.jsonl")), "{}\n");
        let mut l = Locator::new(t.path().to_path_buf());
        assert_eq!(l.transcript(ID_B), Some(t.path().join("D--y").join(format!("{ID_B}.jsonl"))));
        assert_eq!(l.transcript(ID_A), Some(t.path().join("d--x").join(format!("{ID_A}.jsonl"))));
        assert_eq!(l.transcript("cccccccc-1111-4222-8333-444444444444"), None);
    }

    #[test]
    fn finds_transcript_under_non_ascii_project_dir() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("C--demo-B-t-e").join(format!("{ID_A}.jsonl"));
        write(&p, "{\"cwd\":\"C:\\demo\\Bütçe\"}\n");
        let mut l = Locator::new(t.path().to_path_buf());
        let found = l.transcript(ID_A).unwrap();
        assert_eq!(found, p);
        assert!(fs::read_to_string(found).unwrap().contains("Bütçe"));
    }

    #[test]
    fn cached_path_invalidated_when_missing() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("d--x").join(format!("{ID_A}.jsonl"));
        write(&p, "{}\n");
        let mut l = Locator::new(t.path().to_path_buf());
        assert_eq!(l.transcript(ID_A), Some(p.clone()));
        fs::remove_file(&p).unwrap();
        assert_eq!(l.transcript(ID_A), None);
        // moved to another project dir: found again
        let q = t.path().join("D--z").join(format!("{ID_A}.jsonl"));
        write(&q, "{}\n");
        assert_eq!(l.transcript(ID_A), Some(q));
    }

    #[test]
    fn claude_dir_respects_env() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let old = std::env::var_os("CLAUDE_CONFIG_DIR");
        std::env::set_var("CLAUDE_CONFIG_DIR", "some-custom-dir");
        assert_eq!(claude_dir(), PathBuf::from("some-custom-dir"));
        std::env::set_var("CLAUDE_CONFIG_DIR", "");
        assert!(claude_dir().ends_with(".claude"));
        match old {
            Some(v) => std::env::set_var("CLAUDE_CONFIG_DIR", v),
            None => std::env::remove_var("CLAUDE_CONFIG_DIR"),
        }
    }

    #[test]
    fn rejects_non_uuid_session_id() {
        let t = tempfile::tempdir().unwrap();
        // projects dir does not exist: a lookup that touched it would still be None,
        // so also plant a file a traversal could reach.
        write(&t.path().join("x.jsonl"), "{}\n");
        let proj = t.path().join("projects");
        fs::create_dir_all(&proj).unwrap();
        let mut l = Locator::new(proj);
        for bad in ["../../x", "a/b", "", "../x", "AAAAAAAA-1111-4222-8333-444444444444", "aaaaaaaa-1111-4222-8333-44444444444"] {
            assert_eq!(l.transcript(bad), None, "{bad}");
            assert_eq!(l.subagents_dir(bad), None, "{bad}");
        }
    }

    #[test]
    fn subagents_dir_and_all_transcripts() {
        let t = tempfile::tempdir().unwrap();
        write(&t.path().join("d--x").join(format!("{ID_A}.jsonl")), "{}\n");
        fs::create_dir_all(t.path().join("d--x").join(ID_A).join("subagents")).unwrap();
        write(&t.path().join("d--x").join(ID_A).join("subagents").join("agent-1.jsonl"), "{}\n");
        write(&t.path().join("D--y").join(format!("{ID_B}.jsonl")), "{}\n");
        write(&t.path().join("D--y").join("notes.txt"), "x");
        let mut l = Locator::new(t.path().to_path_buf());
        assert_eq!(l.subagents_dir(ID_A), Some(t.path().join("d--x").join(ID_A).join("subagents")));
        assert_eq!(l.subagents_dir(ID_B), None);
        assert_eq!(l.all_transcripts().len(), 2);
    }
}
