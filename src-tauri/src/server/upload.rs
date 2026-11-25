//! Key-authorized file uploads (PUT /upload?dir=&name=).

use std::fs::{self, File};
use std::io::{BufRead, Write};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::http::{Request, Response, escape_html, query_pairs, query_param, split_target};
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
) -> Response {
    let locale = i18n::from_request(request);
    let config = ctx
        .upload
        .read()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    if !config.enabled || config.key.is_empty() {
        return fail(403, locale, "upload.err.disabled");
    }

    if let Some(seconds) = ctx.upload_limiter.blocked_for(peer) {
        return fail(429, locale, "upload.err.too_many").header("Retry-After", seconds.to_string());
    }

    let supplied = request
        .header("x-upload-key")
        .map(str::to_string)
        .or_else(|| query_param(&request.target, "key"));
    let allowed =
        supplied.is_some_and(|key| constant_time_eq(key.as_bytes(), config.key.as_bytes()));
    if !allowed {
        ctx.upload_limiter.note_failure(peer);
        return fail(403, locale, "upload.err.wrong_key");
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
        return fail(400, locale, "upload.err.bad_name");
    };

    let Some(first) = ctx.mounts.first() else {
        return fail(404, locale, "upload.err.no_folder");
    };
    let directory = first.dir.join("uploads");
    if fs::create_dir_all(&directory).is_err() {
        return fail(500, locale, "upload.err.write");
    }

    if request.header("transfer-encoding").is_some() {
        return fail(411, locale, "upload.err.length");
    }
    let Some(length) = request
        .header("content-length")
        .and_then(|value| value.parse::<u64>().ok())
    else {
        return fail(411, locale, "upload.err.length");
    };
    if length == 0 {
        return fail(400, locale, "upload.err.empty");
    }
    if length > config.max_bytes {
        return fail(413, locale, "upload.err.too_large");
    }

    let target = unique_target(&directory, &name);
    let temp = temp_path(&directory);
    let mut file = match File::create_new(&temp) {
        Ok(file) => file,
        Err(_) => return fail(500, locale, "upload.err.write"),
    };

    let result = copy_body(reader, &mut file, length);
    if result.is_err() {
        drop(file);
        let _ = fs::remove_file(&temp);
        return fail(500, locale, "upload.err.interrupted");
    }
    if file.sync_all().is_err() {
        drop(file);
        let _ = fs::remove_file(&temp);
        return fail(500, locale, "upload.err.write");
    }
    drop(file);

    if fs::rename(&temp, &target).is_err() {
        let _ = fs::remove_file(&temp);
        return fail(500, locale, "upload.err.write");
    }

    let saved = target
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or(name);
    let body = format!(
        "{{\"name\":{},\"size\":{length}}}",
        serde_json_string(&saved)
    );
    Response::bytes(201, "application/json; charset=utf-8", body.into_bytes())
        .header("Cache-Control", "no-store")
}

fn copy_body<R: BufRead>(reader: &mut R, file: &mut File, length: u64) -> std::io::Result<()> {
    let mut remaining = length;
    let mut buffer = vec![0u8; 256 * 1024];
    while remaining > 0 {
        let want = remaining.min(buffer.len() as u64) as usize;
        let read = reader.read(&mut buffer[..want])?;
        if read == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "client disconnected",
            ));
        }
        file.write_all(&buffer[..read])?;
        remaining -= read as u64;
    }
    file.flush()
}

pub fn safe_file_name(raw: &str) -> Option<String> {
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

fn unique_target(directory: &Path, name: &str) -> PathBuf {
    let candidate = directory.join(name);
    if fs::symlink_metadata(&candidate).is_err() {
        return candidate;
    }
    let path = Path::new(name);
    let stem = path
        .file_stem()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_default();
    let extension = path
        .extension()
        .map(|value| format!(".{}", value.to_string_lossy()))
        .unwrap_or_default();
    for index in 1..10_000 {
        let candidate = directory.join(format!("{stem} ({index}){extension}"));
        if fs::symlink_metadata(&candidate).is_err() {
            return candidate;
        }
    }
    directory.join(format!("{stem}-{}{extension}", std::process::id()))
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_path(directory: &Path) -> PathBuf {
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    directory.join(format!(
        ".lantransport-{}-{counter}.part",
        std::process::id()
    ))
}

pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn serde_json_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into())
}

fn fail(status: u16, locale: &str, key: &str) -> Response {
    let message = i18n::t(locale, key);
    let body = format!(
        "<!doctype html><html lang=\"{}\"><meta charset=\"utf-8\"><title>Upload</title><body style=\"font:15px system-ui;padding:40px\"><h2>{status}</h2><p>{}</p></body></html>",
        escape_html(locale),
        escape_html(&message)
    );
    Response::html(status, body).header("Cache-Control", "no-store")
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
}
