//! Map a decoded URL path to a real file inside the shared root.

use std::path::{Path, PathBuf};

pub fn resolve(root: &Path, url_path: &str) -> Option<PathBuf> {
    if url_path.contains('\0') {
        return None;
    }
    let mut target = PathBuf::from(root);
    for segment in url_path.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." || segment.starts_with('.') {
            return None;
        }
        if cfg!(windows) && (segment.contains('\\') || segment.contains(':')) {
            return None;
        }
        if !cfg!(windows) && segment.contains('\\') {
            return None;
        }
        target.push(segment);
    }

    let canonical = std::fs::canonicalize(&target).ok()?;
    if canonical == root || canonical.starts_with(root) {
        Some(canonical)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lantransport-test-{}", std::process::id()));
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

    #[test]
    fn rejects_symlink_escape() {
        let root = root();
        #[cfg(unix)]
        {
            let _ = std::fs::remove_file(root.join("escape"));
            std::os::unix::fs::symlink("/etc", root.join("escape")).unwrap();
            assert!(resolve(&root, "/escape/passwd").is_none());
        }
    }
}
