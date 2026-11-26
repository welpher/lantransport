//! Key-authorized file uploads (PUT /upload?dir=&name=).

use std::fs::{self, File};
use std::io::{BufRead, Write};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::http::{Request, Response, query_pairs, query_param, split_target};
use super::{ServerContext, i18n};

#[derive(Clone, Debug)]
pub struct UploadConfig {
    pub enabled: bool,
    pub key: String,
    pub max_bytes: u64,
}

impl Default for UploadConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            key: String::new(),
            max_bytes: 4 * 1024 * 1024 * 1024,
        }
    }
}

pub fn handle<R: BufRead>(
    reader: &mut R,
    request: &Request,
    ctx: &ServerContext,
    peer: IpAddr,
) -> (Response, bool) {
    let locale = i18n::from_request(request);
    let config = ctx
        .upload
        .read()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    if !config.enabled || config.key.is_empty() {
        return (fail(403, locale, "upload.err.disabled"), false);
    }

    if let Some(seconds) = ctx.upload_limiter.blocked_for(peer) {
        return (
            fail(429, locale, "upload.err.too_many").header("Retry-After", seconds.to_string()),
            false,
        );
    }

    let supplied = request
        .header("x-upload-key")
        .map(str::to_string)
        .or_else(|| query_param(&request.target, "key"));
    let allowed = supplied
        .is_some_and(|key| super::auth::constant_time_eq(key.as_bytes(), config.key.as_bytes()));
    if !allowed {
        ctx.upload_limiter.note_failure(peer);
        super::log_event(ctx, &format!("upload denied peer={peer} reason=wrong_key"));
        return (fail(403, locale, "upload.err.wrong_key"), false);
    }
    ctx.upload_limiter.note_success(peer);

    let (_, raw_query) = split_target(&request.target);
    let query = query_pairs(raw_query);
    let Some(name) = query
        .iter()
        .find(|(name, _)| name == "name")
        .map(|(_, value)| value.as_str())
        .and_then(safe_file_name)
    else {
        return (fail(400, locale, "upload.err.bad_name"), false);
    };

    let Some(first) = ctx.mounts.first() else {
        return (fail(404, locale, "upload.err.no_folder"), false);
    };
    let directory = first.dir.join("uploads");
    if fs::create_dir_all(&directory).is_err() {
        return (fail(500, locale, "upload.err.write"), false);
    }

    let chunked = request
        .header("transfer-encoding")
        .is_some_and(|value| value.to_ascii_lowercase().contains("chunked"));

    let declared = if chunked {
        None
    } else {
        let Some(length) = request
            .header("content-length")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return (fail(411, locale, "upload.err.length"), false);
        };
        if length == 0 {
            return (fail(400, locale, "upload.err.empty"), false);
        }
        if length > config.max_bytes {
            return (fail(413, locale, "upload.err.too_large"), false);
        }
        Some(length)
    };

    super::log_event(
        ctx,
        &format!(
            "upload start peer={peer} name=\"{}\" te={:?} cl={:?} ua=\"{}\"",
            name,
            request.header("transfer-encoding"),
            request.header("content-length"),
            request.header("user-agent").unwrap_or("-")
        ),
    );

    let temp = temp_path(&directory);
    let mut file = match File::create_new(&temp) {
        Ok(file) => file,
        Err(_) => return (fail(500, locale, "upload.err.write"), false),
    };

    let written = match declared {
        Some(length) => match copy_body(reader, &mut file, length) {
            Ok(written) => written,
            Err((written, error)) => {
                drop(file);
                let _ = fs::remove_file(&temp);
                super::log_event(
                    ctx,
                    &format!(
                        "upload failed peer={peer} name=\"{name}\" wrote={written}/{length} error={error}"
                    ),
                );
                return (fail(500, locale, "upload.err.interrupted"), false);
            }
        },
        None => match copy_chunked(reader, &mut file, config.max_bytes) {
            Ok(written) => written,
            Err(BodyError::TooLarge) => {
                drop(file);
                let _ = fs::remove_file(&temp);
                return (fail(413, locale, "upload.err.too_large"), false);
            }
            Err(BodyError::Io(error)) => {
                drop(file);
                let _ = fs::remove_file(&temp);
                super::log_event(
                    ctx,
                    &format!("upload failed peer={peer} name=\"{name}\" chunked error={error}"),
                );
                return (fail(500, locale, "upload.err.interrupted"), false);
            }
        },
    };

    if file.sync_all().is_err() {
        drop(file);
        let _ = fs::remove_file(&temp);
        return (fail(500, locale, "upload.err.write"), false);
    }
    drop(file);

    let Some(target) = commit(&temp, &directory, &name) else {
        let _ = fs::remove_file(&temp);
        return (fail(500, locale, "upload.err.write"), false);
    };

    let saved = target
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or(name);
    super::log_event(
        ctx,
        &format!("upload ok peer={peer} name=\"{saved}\" bytes={written}"),
    );
    let body = format!(
        "{{\"name\":{},\"size\":{written}}}",
        serde_json_string(&saved)
    );
    (
        Response::bytes(201, "application/json; charset=utf-8", body.into_bytes())
            .header("Cache-Control", "no-store"),
        true,
    )
}

type BodyResult = Result<u64, (u64, std::io::Error)>;

fn copy_body<R: BufRead>(reader: &mut R, file: &mut File, length: u64) -> BodyResult {
    let mut remaining = length;
    let mut written = 0u64;
    let mut buffer = vec![0u8; 256 * 1024];
    while remaining > 0 {
        let want = remaining.min(buffer.len() as u64) as usize;
        let read = match reader.read(&mut buffer[..want]) {
            Ok(0) => {
                return Err((
                    written,
                    std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "client disconnected"),
                ));
            }
            Ok(read) => read,
            Err(error) => return Err((written, error)),
        };
        if let Err(error) = file.write_all(&buffer[..read]) {
            return Err((written, error));
        }
        written += read as u64;
        remaining -= read as u64;
    }
    file.flush().map_err(|error| (written, error))?;
    Ok(written)
}

pub enum BodyError {
    TooLarge,
    Io(std::io::Error),
}

fn copy_chunked<R: BufRead>(
    reader: &mut R,
    file: &mut File,
    max_bytes: u64,
) -> Result<u64, BodyError> {
    let mut total = 0u64;
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let line = read_line(reader, 1024).map_err(BodyError::Io)?;
        let size_text = line.split(';').next().unwrap_or_default().trim();
        let size = u64::from_str_radix(size_text, 16).map_err(|_| {
            BodyError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "bad chunk size",
            ))
        })?;
        if size == 0 {
            loop {
                let trailer = read_line(reader, 8 * 1024).map_err(BodyError::Io)?;
                if trailer.is_empty() {
                    return Ok(total);
                }
            }
        }
        total = total.saturating_add(size);
        if total > max_bytes {
            return Err(BodyError::TooLarge);
        }
        let mut remaining = size;
        while remaining > 0 {
            let want = remaining.min(buffer.len() as u64) as usize;
            let read = reader.read(&mut buffer[..want]).map_err(BodyError::Io)?;
            if read == 0 {
                return Err(BodyError::Io(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "client disconnected",
                )));
            }
            file.write_all(&buffer[..read]).map_err(BodyError::Io)?;
            remaining -= read as u64;
        }
        let _terminator = read_line(reader, 8).map_err(BodyError::Io)?;
    }
}

fn read_line<R: BufRead>(reader: &mut R, max: usize) -> std::io::Result<String> {
    let mut line = Vec::new();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            if line.is_empty() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "client disconnected",
                ));
            }
            break;
        }
        let (done, used) = match available.iter().position(|byte| *byte == b'\n') {
            Some(index) => (true, index + 1),
            None => (false, available.len()),
        };
        if line.len() + used > max {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "line too long",
            ));
        }
        line.extend_from_slice(&available[..used]);
        reader.consume(used);
        if done {
            break;
        }
    }
    Ok(String::from_utf8_lossy(&line)
        .trim_end_matches(['\r', '\n'])
        .to_string())
}

fn safe_file_name(raw: &str) -> Option<String> {
    let name = raw.trim();
    if name.is_empty() || name.len() > 255 {
        return None;
    }
    if name.starts_with('.') || name.contains('/') || name.contains('\\') {
        return None;
    }
    if name.chars().any(char::is_control) {
        return None;
    }
    if cfg!(windows) {
        let name = name.trim_end_matches(['.', ' ']);
        if name.is_empty() || name.contains(':') {
            return None;
        }
        let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
        const RESERVED: [&str; 22] = [
            "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
            "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
        ];
        if RESERVED.contains(&stem.as_str()) {
            return None;
        }
        return Some(name.to_string());
    }
    Some(name.to_string())
}

/// Move the finished temp file into place without ever replacing an existing
/// name. `hard_link` fails with `AlreadyExists` when the name is taken, so the
/// claim is atomic and two concurrent uploads of the same name get distinct
/// paths. Filesystems without hard links (exFAT, FAT32, ...) fall back to a
/// `create_new` claim plus a copy.
fn commit(temp: &Path, directory: &Path, name: &str) -> Option<PathBuf> {
    let path = Path::new(name);
    let stem = path
        .file_stem()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_default();
    let extension = path
        .extension()
        .map(|value| format!(".{}", value.to_string_lossy()))
        .unwrap_or_default();
    let mut supports_links = true;
    for index in 0..10_000 {
        let candidate = if index == 0 {
            directory.join(name)
        } else {
            directory.join(format!("{stem} ({index}){extension}"))
        };
        if supports_links {
            match fs::hard_link(temp, &candidate) {
                Ok(()) => {
                    let _ = fs::remove_file(temp);
                    return Some(candidate);
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(_) => supports_links = false,
            }
        }
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(handle) => {
                drop(handle);
                if fs::copy(temp, &candidate).is_err() {
                    let _ = fs::remove_file(&candidate);
                    return None;
                }
                let _ = fs::remove_file(temp);
                return Some(candidate);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return None,
        }
    }
    None
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_path(directory: &Path) -> PathBuf {
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    directory.join(format!(
        ".lantransport-{}-{counter}.part",
        std::process::id()
    ))
}

fn serde_json_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into())
}

fn fail(status: u16, locale: &str, key: &str) -> Response {
    let body = serde_json::json!({
        "error": key,
        "message": i18n::t(locale, key),
    })
    .to_string();
    Response::bytes(status, "application/json; charset=utf-8", body.into_bytes())
        .header("Cache-Control", "no-store")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsafe_names() {
        assert!(safe_file_name("photo.jpg").is_some());
        assert!(safe_file_name("../evil").is_none());
        assert!(safe_file_name("a/b").is_none());
        assert!(safe_file_name("a\\b").is_none());
        assert!(safe_file_name(".hidden").is_none());
        assert!(safe_file_name("").is_none());
        assert!(safe_file_name("bad\nname").is_none());
        assert!(safe_file_name("报告.pdf").is_some());
    }

    #[test]
    fn parses_query() {
        let values = query_pairs("dir=%2Fsub%2F&name=a+b.txt&key=x");
        assert_eq!(values[0], ("dir".into(), "/sub/".into()));
        assert_eq!(values[1], ("name".into(), "a b.txt".into()));
        assert_eq!(values[2], ("key".into(), "x".into()));
    }

    fn scratch_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("lantransport-commit-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn commit_never_replaces_an_existing_name() {
        let dir = scratch_dir("collide");
        fs::write(dir.join("note.txt"), b"original").unwrap();
        let temp = temp_path(&dir);
        fs::write(&temp, b"newcomer").unwrap();

        let saved = commit(&temp, &dir, "note.txt").expect("committed");
        assert_eq!(saved.file_name().unwrap(), "note (1).txt");
        assert_eq!(
            fs::read_to_string(dir.join("note.txt")).unwrap(),
            "original"
        );
        assert_eq!(fs::read_to_string(&saved).unwrap(), "newcomer");
        assert!(!temp.exists(), "temp file must be cleaned up");
    }

    #[test]
    fn concurrent_commits_each_keep_their_bytes() {
        let dir = scratch_dir("race");
        let payloads: Vec<String> = (0..16).map(|tag| format!("{tag:0>64}")).collect();
        let handles: Vec<_> = payloads
            .iter()
            .cloned()
            .map(|payload| {
                let dir = dir.clone();
                std::thread::spawn(move || {
                    let temp = temp_path(&dir);
                    fs::write(&temp, payload.as_bytes()).unwrap();
                    commit(&temp, &dir, "race.txt")
                })
            })
            .collect();

        let mut saved = Vec::new();
        for handle in handles {
            let path = handle.join().unwrap().expect("commit");
            saved.push(fs::read_to_string(path).unwrap());
        }
        saved.sort();
        let mut expected = payloads;
        expected.sort();
        assert_eq!(saved, expected, "no upload may be overwritten");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 16);
    }
}
