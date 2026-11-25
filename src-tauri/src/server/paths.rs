//! Map decoded URL paths onto shared roots without ever escaping them.

use std::path::{Path, PathBuf};

pub fn segments(url_path: &str) -> Option<Vec<String>> {
    if url_path.contains('\0') {
        return None;
    }
    let mut segments = Vec::new();
    for segment in url_path.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." || segment.starts_with('.') || segment.contains('\\') {
            return None;
        }
        if cfg!(windows) && segment.contains(':') {
            return None;
        }
        segments.push(segment.to_string());
    }
    Some(segments)
}

pub fn resolve_in(root: &Path, segments: &[String]) -> Option<PathBuf> {
    let mut target = root.to_path_buf();
    for segment in segments {
        target.push(segment);
    }
    let canonical = std::fs::canonicalize(&target).ok()?;
    if canonical == root || canonical.starts_with(root) {
        Some(canonical)
    } else {
        None
    }
}

pub fn resolve(root: &Path, url_path: &str) -> Option<PathBuf> {
    resolve_in(root, &segments(url_path)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lantransport-paths-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub/file.txt"), b"hi").unwrap();
        std::fs::write(dir.join(".hidden"), b"no").unwrap();
        std::fs::canonicalize(dir).unwrap()
    }

    #[test]
    fn stays_inside_root() {
        let root = root();
        assert!(resolve(&root, "/sub/file.txt").is_some());
        assert!(resolve(&root, "/").is_some());
        assert!(resolve(&root, "/../etc/passwd").is_none());
        assert!(resolve(&root, "/sub/../../etc/passwd").is_none());
        assert!(resolve(&root, "/.hidden").is_none());
        assert!(resolve(&root, "/sub/./file.txt").is_some());
        assert!(resolve(&root, "/missing").is_none());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape() {
        let root = root();
        let _ = std::fs::remove_file(root.join("escape"));
        std::os::unix::fs::symlink("/etc", root.join("escape")).unwrap();
        assert!(resolve(&root, "/escape/passwd").is_none());
    }
}
