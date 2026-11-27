//! Key-authorized file uploads (PUT /upload?dir=&name=).

use std::fs::{self, File};
use std::io::{BufRead, Write};
use std::net::IpAddr;
use std::path::{Path, PathBuf};

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

/// File name plus the key that identifies a resumable upload: the whole relative
/// path for a folder upload, the plain name otherwise.
fn destination_name(query: &[(String, String)]) -> Result<(String, String), &'static str> {
    if let Some(raw) = relative_path(query) {
        let (_, name) = split_relative(raw).ok_or("upload.err.bad_path")?;
        return Ok((name, raw.to_string()));
    }
    query
        .iter()
        .find(|(key, _)| key == "name")
        .map(|(_, value)| value.as_str())
        .and_then(safe_file_name)
        .map(|name| (name.clone(), name))
        .ok_or("upload.err.bad_name")
}

/// The `path` parameter of a folder upload, when there is one.
fn relative_path(query: &[(String, String)]) -> Option<&str> {
    query
        .iter()
        .find(|(key, _)| key == "path")
        .map(|(_, value)| value.as_str())
        .filter(|value| !value.is_empty())
}

/// The id the page gives to every file of one drop. Kept to a small alphabet so
/// it is safe to log and to key a map with.
fn batch_id(query: &[(String, String)]) -> Option<String> {
    let raw = query
        .iter()
        .find(|(key, _)| key == "batch")
        .map(|(_, value)| value.as_str())
        .filter(|value| !value.is_empty() && value.len() <= 64)?;
    if !raw
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return None;
    }
    Some(raw.to_string())
}

/// Where a finished upload lands: the folder the visitor is looking at, or the
/// legacy `uploads/` folder of the first share for pages that do not ask.
fn upload_target(ctx: &ServerContext, query: &[(String, String)]) -> Option<PathBuf> {
    let requested = query
        .iter()
        .find(|(name, _)| name == "dir")
        .map(|(_, value)| value.as_str());
    match requested {
        None => ctx.mounts.first().map(|mount| mount.dir.join("uploads")),
        Some(value) => {
            let segments = super::paths::segments(value)?;
            ctx.mounts.iter().find_map(|mount| {
                super::paths::resolve_in(&mount.dir, &segments).filter(|path| path.is_dir())
            })
        }
    }
}

/// `HEAD /upload?dir=&name=` answers how much of that file already arrived, so a
/// dropped connection can continue instead of starting over.
pub fn probe(request: &Request, ctx: &ServerContext) -> Response {
    let locale = i18n::from_request(request);
    let config = ctx
        .upload
        .read()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    if !config.enabled || config.key.is_empty() {
        return fail(403, locale, "upload.err.disabled");
    }
    let supplied = request
        .header("x-upload-key")
        .map(str::to_string)
        .or_else(|| query_param(&request.target, "key"));
    let allowed = supplied
        .is_some_and(|key| super::auth::constant_time_eq(key.as_bytes(), config.key.as_bytes()));
    if !allowed {
        return fail(403, locale, "upload.err.wrong_key");
    }
    let (_, raw_query) = split_target(&request.target);
    let query = query_pairs(raw_query);
    let (_, resume_key) = match destination_name(&query) {
        Ok(found) => found,
        Err(key) => return fail(400, locale, key),
    };
    let directory = match upload_place(ctx, &query) {
        Ok((_, directory)) => directory,
        Err(key) => {
            let status = if key == "upload.err.no_folder" {
                404
            } else {
                400
            };
            return fail(status, locale, key);
        }
    };
    let offset = fs::metadata(part_path(&directory, &resume_key))
        .map(|meta| meta.len())
        .unwrap_or(0);
    Response::empty(204)
        .header("X-Upload-Offset", offset.to_string())
        .header("Cache-Control", "no-store")
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

    // A page that sends a whole folder marks every file with a batch id and then
    // closes the batch with `end=1`, so the app can show one entry and one
    // notification instead of one per file.
    let batch = batch_id(&query);
    if query_param(&request.target, "end").is_some_and(|value| value == "1") {
        let Some(id) = batch else {
            return (fail(400, locale, "upload.err.bad_name"), false);
        };
        let label = query
            .iter()
            .find(|(key, _)| key == "blabel")
            .map(|(_, value)| value.as_str())
            .and_then(safe_file_name);
        super::log_event(
            ctx,
            &format!("upload batch end peer={peer} id=\"{id}\" label={label:?}"),
        );
        if let Some(hook) = &ctx.on_batch_end {
            hook(super::UploadBatch { id, label, peer });
        }
        return (
            Response::empty(204).header("Cache-Control", "no-store"),
            true,
        );
    }

    let (name, resume_key) = match destination_name(&query) {
        Ok(found) => found,
        Err(key) => return (fail(400, locale, key), false),
    };
    let directory = match upload_place(ctx, &query) {
        Ok((_, directory)) => directory,
        Err(key) => {
            let status = if key == "upload.err.no_folder" {
                404
            } else {
                400
            };
            return (fail(status, locale, key), false);
        }
    };

    // An empty folder in a dragged tree has no file to carry it, so the page
    // asks for the folders themselves with `mkdir=1`.
    if query_param(&request.target, "mkdir").is_some_and(|value| value == "1") {
        let relative = relative_path(&query).unwrap_or_default();
        match make_folder(ctx, &query) {
            Ok(created) => super::log_event(
                ctx,
                &format!("mkdir peer={peer} path=\"{relative}\" dir=\"{created:?}\""),
            ),
            Err(key) => return (fail(400, locale, key), false),
        }
        let has_body = request
            .header("content-length")
            .and_then(|value| value.trim().parse::<u64>().ok())
            .is_some_and(|length| length > 0);
        return (
            Response::empty(201).header("Cache-Control", "no-store"),
            !has_body,
        );
    }
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

    // Resume: the page probes with HEAD first and then sends only the missing
    // tail with X-Upload-Offset / X-Upload-Total.
    let offset = request
        .header("x-upload-offset")
        .and_then(|value| value.trim().parse::<u64>().ok());
    let total = request
        .header("x-upload-total")
        .and_then(|value| value.trim().parse::<u64>().ok());

    super::log_event(
        ctx,
        &format!(
            "upload start peer={peer} name=\"{}\" batch={:?} te={:?} cl={:?} offset={offset:?} total={total:?} ua=\"{}\"",
            name,
            batch,
            request.header("transfer-encoding"),
            request.header("content-length"),
            request.header("user-agent").unwrap_or("-")
        ),
    );

    let temp = part_path(&directory, &resume_key);
    let mut file = match open_part(&temp, offset) {
        Ok(Some(file)) => file,
        Ok(None) => {
            let actual = fs::metadata(&temp).map(|meta| meta.len()).unwrap_or(0);
            return (
                fail(409, locale, "upload.err.offset")
                    .header("X-Upload-Offset", actual.to_string()),
                false,
            );
        }
        Err(_) => return (fail(500, locale, "upload.err.write"), false),
    };

    let written = match declared {
        Some(length) => match copy_body(reader, &mut file, length) {
            Ok(written) => written,
            Err((written, error)) => {
                // Keep the part file: the next attempt resumes from here.
                drop(file);
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
                super::log_event(
                    ctx,
                    &format!("upload failed peer={peer} name=\"{name}\" chunked error={error}"),
                );
                return (fail(500, locale, "upload.err.interrupted"), false);
            }
        },
    };

    let written = written + offset.unwrap_or(0);
    if let Some(total) = total
        && written != total
    {
        drop(file);
        super::log_event(
            ctx,
            &format!("upload incomplete peer={peer} name=\"{name}\" {written}/{total}"),
        );
        return (
            fail(400, locale, "upload.err.incomplete")
                .header("X-Upload-Offset", written.to_string()),
            true,
        );
    }

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
        &format!("upload ok peer={peer} name=\"{saved}\" batch={batch:?} bytes={written}"),
    );
    if let Some(hook) = &ctx.on_upload {
        hook(super::UploadNotice {
            name: saved.clone(),
            bytes: written,
            peer,
            dir: directory.clone(),
            batch,
        });
    }
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

/// Splits `a/b/c.jpg` into the directory chain and the file name. Every segment
/// has to pass the same rules as a single file name, so `..`, absolute paths,
/// hidden names and Windows reserved names are all rejected.
fn split_relative(raw: &str) -> Option<(Vec<String>, String)> {
    const MAX_DEPTH: usize = 16;
    const MAX_LEN: usize = 1024;
    if raw.len() > MAX_LEN
        || raw.is_empty()
        || raw.contains('\\')
        || raw.starts_with('/')
        || raw.contains('\0')
    {
        return None;
    }
    let segments: Vec<&str> = raw.split('/').filter(|part| !part.is_empty()).collect();
    if segments.is_empty() || segments.len() > MAX_DEPTH {
        return None;
    }
    let mut dirs = Vec::with_capacity(segments.len().saturating_sub(1));
    for segment in &segments[..segments.len() - 1] {
        dirs.push(safe_file_name(segment)?);
    }
    let name = safe_file_name(segments[segments.len() - 1])?;
    Some((dirs, name))
}

/// Resolves the upload destination: the folder the visitor is looking at, plus
/// any subfolders from a folder upload. New folders are created and then
/// canonicalised again, so a symlink planted inside a share cannot redirect the
/// write somewhere else.
fn upload_place(
    ctx: &ServerContext,
    query: &[(String, String)],
) -> Result<(PathBuf, PathBuf), &'static str> {
    let base = upload_target(ctx, query).ok_or("upload.err.no_folder")?;
    let Some(raw) = relative_path(query) else {
        return Ok((base.clone(), base));
    };
    let (dirs, _) = split_relative(raw).ok_or("upload.err.bad_path")?;
    let target = ensure_inside(&base, &dirs)?;
    Ok((base, target))
}

/// Creates every folder along `path` and returns the deepest one, so a dragged
/// tree that contains an empty folder still shows up on the page.
fn make_folder(ctx: &ServerContext, query: &[(String, String)]) -> Result<PathBuf, &'static str> {
    let base = upload_target(ctx, query).ok_or("upload.err.no_folder")?;
    let (mut segments, name) =
        split_relative(relative_path(query).unwrap_or_default()).ok_or("upload.err.bad_path")?;
    segments.push(name);
    ensure_inside(&base, &segments)
}

/// Joins `segments` onto `base`, creates the folders, then canonicalises both
/// and checks the result is still below `base`: a symlink in the share could
/// otherwise point the write at a folder outside of it.
fn ensure_inside(base: &Path, segments: &[String]) -> Result<PathBuf, &'static str> {
    let mut target = base.to_path_buf();
    for segment in segments {
        target.push(segment);
    }
    fs::create_dir_all(&target).map_err(|_| "upload.err.bad_path")?;
    let base = fs::canonicalize(base).map_err(|_| "upload.err.no_folder")?;
    let target = fs::canonicalize(&target).map_err(|_| "upload.err.bad_path")?;
    if !target.starts_with(&base) {
        return Err("upload.err.bad_path");
    }
    Ok(target)
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

/// One part file per (folder, name): a retry after a dropped connection finds
/// the bytes that made it and continues from there.
fn part_path(directory: &Path, name: &str) -> PathBuf {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in name.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    directory.join(format!(".lantransport-{hash:016x}.part"))
}

/// `Ok(Some(file))` to append from `offset`, `Ok(None)` when the offset does not
/// match what is already on disk, `Err` when the file cannot be opened.
fn open_part(temp: &Path, offset: Option<u64>) -> std::io::Result<Option<File>> {
    match offset {
        Some(offset) => {
            let existing = fs::metadata(temp).map(|meta| meta.len()).unwrap_or(0);
            if existing != offset {
                return Ok(None);
            }
            let file = fs::OpenOptions::new().append(true).open(temp)?;
            Ok(Some(file))
        }
        None => {
            // Fresh start: drop whatever an earlier attempt left behind.
            let _ = fs::remove_file(temp);
            Ok(Some(File::create_new(temp)?))
        }
    }
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

    #[test]
    fn splits_relative_paths_only_when_every_segment_is_safe() {
        assert_eq!(
            split_relative("a/b/c.jpg"),
            Some((vec!["a".into(), "b".into()], "c.jpg".into()))
        );
        assert_eq!(split_relative("c.jpg"), Some((Vec::new(), "c.jpg".into())));
        // Empty segments come from a trailing slash or a double slash.
        assert_eq!(
            split_relative("a//b.jpg"),
            Some((vec!["a".into()], "b.jpg".into()))
        );
        assert!(split_relative("").is_none());
        assert!(split_relative("a/../c.jpg").is_none());
        assert!(split_relative("../c.jpg").is_none());
        assert!(split_relative("/etc/passwd").is_none());
        assert!(split_relative("a\\b.jpg").is_none());
        assert!(split_relative("a/.hidden/c.jpg").is_none());
        assert!(split_relative("a/b\0.jpg").is_none());
        let deep = vec!["d"; 17].join("/");
        assert!(split_relative(&deep).is_none(), "depth is capped");
        let long = format!("{}.txt", "n".repeat(300));
        assert!(split_relative(&long).is_none(), "length is capped");
    }

    #[test]
    fn destination_names_use_the_whole_path_for_folder_uploads() {
        let folder = query_pairs("path=a%2Fb%2Fnote.txt&name=note.txt");
        assert_eq!(
            destination_name(&folder),
            Ok(("note.txt".into(), "a/b/note.txt".into()))
        );
        let plain = query_pairs("name=note.txt");
        assert_eq!(
            destination_name(&plain),
            Ok(("note.txt".into(), "note.txt".into()))
        );
        assert_eq!(
            destination_name(&query_pairs("path=..%2Fx")),
            Err("upload.err.bad_path")
        );
        assert_eq!(
            destination_name(&query_pairs("path=")),
            Err("upload.err.bad_name")
        );
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
        let temp = dir.join(".part");
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
            .enumerate()
            .map(|(index, payload)| {
                let dir = dir.clone();
                std::thread::spawn(move || {
                    let temp = dir.join(format!(".part-{index}"));
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
