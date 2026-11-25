//! Request routing: listing, file download, auth and error pages.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Local};
use percent_encoding::percent_decode_str;

use super::http::{
    Method, RangeOutcome, Request, Response, content_disposition, escape_html, http_date,
    parse_range, split_target,
};
use super::listing::{self, Entry};
use super::{ServerContext, i18n, paths};

pub fn handle(request: &Request, ctx: &ServerContext, peer: IpAddr) -> Response {
    if let Method::Other(_) = &request.method {
        return Response::empty(405)
            .header("Allow", "GET, HEAD, PUT")
            .header("Content-Type", "text/plain; charset=utf-8");
    }

    let locale = i18n::from_request(request);
    let (raw_path, raw_query) = split_target(&request.target);
    let decoded = percent_decode_str(raw_path)
        .decode_utf8_lossy()
        .into_owned();
    if decoded.contains('\0') {
        return simple(400, locale);
    }

    if decoded == "/favicon.ico" {
        return Response::empty(204);
    }

    if ctx.auth.required() {
        if let Some(seconds) = ctx.limiter.blocked_for(peer) {
            return simple(429, locale).header("Retry-After", seconds.to_string());
        }
        if !ctx.auth.check(request.header("authorization")) {
            ctx.limiter.note_failure(peer);
            return Response::html(401, login_page(locale)).header(
                "WWW-Authenticate",
                "Basic realm=\"LanTransport\", charset=\"UTF-8\"",
            );
        }
        ctx.limiter.note_success(peer);
    }

    let Some(segments) = paths::segments(&decoded) else {
        return simple(404, locale);
    };

    let found: Vec<(usize, PathBuf)> = ctx
        .mounts
        .iter()
        .enumerate()
        .filter_map(|(index, mount)| {
            paths::resolve_in(&mount.dir, &segments).map(|target| (index, target))
        })
        .collect();
    if found.is_empty() {
        return simple(404, locale);
    }

    let dirs: Vec<(usize, PathBuf)> = found
        .iter()
        .filter(|(_, target)| target.is_dir())
        .cloned()
        .collect();

    if !dirs.is_empty() {
        if !decoded.ends_with('/') {
            let mut location = format!("{}/", raw_path.trim_end_matches('/'));
            if !raw_query.is_empty() {
                location.push('?');
                location.push_str(raw_query);
            }
            return Response::empty(301).header("Location", location);
        }
        let uploads = ctx
            .upload
            .read()
            .map(|config| config.enabled && !config.key.is_empty())
            .unwrap_or(false);
        return serve_listing(&ctx.mounts, &dirs, &decoded, locale, uploads);
    }

    let requested_mount = super::http::query_param(&request.target, "src")
        .and_then(|value| value.parse::<usize>().ok());
    let Some((_, target)) = requested_mount
        .and_then(|index| found.iter().find(|(mount, _)| *mount == index))
        .or_else(|| found.first())
    else {
        return simple(404, locale);
    };
    let Ok(metadata) = fs::metadata(target) else {
        return simple(404, locale);
    };
    if !metadata.is_file() {
        return simple(404, locale);
    }
    serve_file(
        target,
        &metadata,
        raw_query.contains("download"),
        request,
        locale,
    )
}

fn serve_listing(
    mounts: &[super::Mount],
    dirs: &[(usize, PathBuf)],
    url_path: &str,
    locale: &str,
    uploads: bool,
) -> Response {
    let mut merged: BTreeMap<String, Entry> = BTreeMap::new();
    let mut readable = false;
    let mut total_size = 0u64;

    for (mount_index, dir) in dirs {
        let Ok(read) = fs::read_dir(dir) else {
            continue;
        };
        readable = true;
        for entry in read.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            let slot = merged.entry(name.clone()).or_insert_with(|| Entry {
                name,
                dir_mounts: Vec::new(),
                files: Vec::new(),
            });
            if file_type.is_dir() {
                if !slot.dir_mounts.contains(mount_index) {
                    slot.dir_mounts.push(*mount_index);
                }
            } else {
                let metadata = entry.metadata().ok();
                let size = metadata.as_ref().map_or(0, |meta| meta.len());
                total_size += size;
                slot.files.push(listing::FileEntry {
                    mount: *mount_index,
                    size,
                    modified: metadata
                        .and_then(|meta| meta.modified().ok())
                        .map(DateTime::<Local>::from),
                    label: mounts[*mount_index].label.clone(),
                });
            }
        }
    }

    if !readable {
        return simple(403, locale);
    }

    let entries: Vec<Entry> = merged.into_values().collect();
    let dir_name = if url_path == "/" {
        i18n::t(locale, "page.all_files")
    } else {
        url_path
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or(url_path)
            .to_string()
    };
    let nonce = nonce();
    let page = listing::render(
        &dir_name, url_path, &entries, total_size, locale, &nonce, uploads,
    );
    Response::html(200, page)
        .header("Cache-Control", "no-store")
        .header(
            "Content-Security-Policy",
            format!(
                "default-src 'none'; style-src 'unsafe-inline'; script-src 'nonce-{nonce}'; img-src data:; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'"
            ),
        )
        .header("Referrer-Policy", "no-referrer")
        .header("X-Content-Type-Options", "nosniff")
}

fn serve_file(
    path: &Path,
    metadata: &fs::Metadata,
    as_attachment: bool,
    request: &Request,
    locale: &str,
) -> Response {
    let size = metadata.len();
    let modified = metadata.modified().ok();
    let etag = format!(
        "\"{:x}-{:x}\"",
        size,
        modified
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |duration| duration.as_secs())
    );

    if request
        .header("if-none-match")
        .is_some_and(|value| value.split(',').any(|tag| tag.trim() == etag))
    {
        return Response::empty(304).header("ETag", &etag);
    }

    let content_type = content_type_for(path);
    let mut response = match parse_range(request.header("range"), size) {
        RangeOutcome::Unsatisfiable => {
            return Response::empty(416).header("Content-Range", format!("bytes */{size}"));
        }
        RangeOutcome::Partial { start, end } => {
            let Ok(file) = File::open(path) else {
                return simple(404, locale);
            };
            let length = end - start + 1;
            Response::file(206, file, start, length)
                .header("Content-Range", format!("bytes {start}-{end}/{size}"))
        }
        RangeOutcome::Full => {
            let Ok(file) = File::open(path) else {
                return simple(404, locale);
            };
            Response::file(200, file, 0, size)
        }
    };

    response = response
        .header("Content-Type", content_type)
        .header("Accept-Ranges", "bytes")
        .header("Cache-Control", "no-cache")
        .header("ETag", etag)
        .header("X-Content-Type-Options", "nosniff")
        .header("Referrer-Policy", "no-referrer");
    if let Some(modified) = modified {
        response = response.header(
            "Last-Modified",
            http_date(DateTime::<Local>::from(modified)),
        );
    }
    if as_attachment {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "download".into());
        response = response.header("Content-Disposition", content_disposition(&name));
    }
    response
}

fn content_type_for(path: &Path) -> String {
    let guessed = mime_guess::from_path(path).first_or_octet_stream();
    let essence = guessed.essence_str();
    if essence.starts_with("text/")
        || matches!(
            essence,
            "application/json" | "application/javascript" | "application/xml" | "image/svg+xml"
        )
    {
        format!("{essence}; charset=utf-8")
    } else {
        essence.to_string()
    }
}

fn simple(status: u16, locale: &str) -> Response {
    let key = match status {
        400 | 403 | 404 | 429 | 500 => format!("error.{status}"),
        _ => "error.500".to_string(),
    };
    let message = i18n::t(locale, &key);
    let page = format!(
        "<!doctype html><html lang=\"{}\"><meta charset=\"utf-8\"><meta name=\"color-scheme\" content=\"dark light\"><title>{status}</title><body style=\"font:15px system-ui;padding:40px\"><h2>{status}</h2><p>{message}</p></body></html>",
        escape_html(locale),
        message = escape_html(&message)
    );
    Response::html(status, page)
}

fn login_page(locale: &str) -> String {
    format!(
        "<!doctype html><html lang=\"{}\"><meta charset=\"utf-8\"><title>401</title><body style=\"font:15px system-ui;padding:40px\"><h2>401</h2><p>{}</p></body></html>",
        escape_html(locale),
        escape_html(&i18n::t(locale, "auth.hint"))
    )
}

static NONCE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn nonce() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u64(NONCE_COUNTER.fetch_add(1, Ordering::Relaxed));
    hasher.write_u128(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos()),
    );
    format!("{:x}{:x}", hasher.finish(), hasher.finish())
}
