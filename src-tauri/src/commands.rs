use std::path::Path;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_notification::NotificationExt;

use crate::server::i18n;
use crate::server::upload::UploadConfig;
use crate::server::{ServerHandle, StartError};
use crate::state::{self, AppState, Snapshot};
use crate::tray;

/// Shortest browsing password the server will accept.
pub const MIN_PASSWORD_CHARS: usize = 4;

pub fn notify(app: &AppHandle) {
    let snapshot = state::snapshot(app);
    let _ = app.emit("snapshot", snapshot);
    let _ = tray::refresh(app);
}

/// Adds dropped folders to the share list. Files are ignored: dropping a file on
/// the window means "share this folder", not "upload it".
fn normalize_shares(shares: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for share in shares {
        let trimmed = share.trim();
        if trimmed.is_empty() || out.iter().any(|kept| kept == trimmed) {
            continue;
        }
        out.push(trimmed.to_string());
    }
    out
}

pub fn add_shared_folders(app: &AppHandle, paths: &[std::path::PathBuf]) -> (usize, usize) {
    let state = app.state::<AppState>();
    let added = {
        let mut settings = state
            .settings
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let mut added = 0;
        let mut known = 0;
        for path in paths {
            if !path.is_dir() {
                continue;
            }
            let text = path.to_string_lossy().into_owned();
            if settings.shares.iter().any(|share| share == &text) {
                known += 1;
                continue;
            }
            settings.shares.push(text);
            added += 1;
        }
        if added > 0 {
            let _ = settings.save(app);
        }
        (added, known)
    };
    notify(app);
    added
}

pub fn error_code(error: &StartError) -> String {
    match error {
        StartError::InvalidRoot => "invalid_root",
        StartError::PortInUse => "port_in_use",
        StartError::PermissionDenied => "permission_denied",
        StartError::Io(_) => "io",
    }
    .to_string()
}

pub fn upload_config(settings: &crate::settings::Settings) -> UploadConfig {
    UploadConfig {
        enabled: settings.allow_uploads && !settings.upload_key.is_empty(),
        key: settings.upload_key.clone(),
        max_bytes: u64::from(settings.max_upload_mb.max(1)) * 1024 * 1024,
    }
}

pub fn start_from_settings(app: &AppHandle) -> Result<Snapshot, String> {
    let state = app.state::<AppState>();
    let (shares, port, password, upload) = {
        let settings = state
            .settings
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        // Enforce the same floor the start dialog uses, so a short password
        // saved via update_password cannot slip through auto-start.
        let password = settings
            .password
            .clone()
            .filter(|value| value.chars().count() >= MIN_PASSWORD_CHARS);
        (
            settings.shares.clone(),
            settings.port,
            password,
            upload_config(&settings),
        )
    };
    if shares.is_empty() {
        return Err("no_folder".into());
    }
    match start_server(app, &shares, port, true, password, upload) {
        Ok(snapshot) => Ok(snapshot),
        Err(code) => {
            record_start_error(app, &code, true);
            Err(code)
        }
    }
}

/// Opens a folder in the system file manager and brings it to the front.
/// `/usr/bin/open` alone can leave Finder behind while the app runs as an
/// accessory (tray) app, so macOS names Finder explicitly.
pub fn open_folder(path: &std::path::Path) -> bool {
    if !path.is_dir() {
        return false;
    }
    #[cfg(target_os = "macos")]
    let spawned = std::process::Command::new("/usr/bin/open")
        .arg("-a")
        .arg("Finder")
        .arg(path)
        .spawn()
        .is_ok();
    #[cfg(target_os = "windows")]
    let spawned = std::process::Command::new("explorer")
        .arg(path)
        .spawn()
        .is_ok();
    #[cfg(all(unix, not(target_os = "macos")))]
    let spawned = std::process::Command::new("xdg-open")
        .arg(path)
        .spawn()
        .is_ok();
    spawned
}

/// Folds one uploaded file into the batch it belongs to.
fn accumulate(batch: &mut crate::state::PendingBatch, notice: &crate::server::UploadNotice) {
    let dir = notice.dir.to_string_lossy().into_owned();
    batch.files += 1;
    batch.bytes += notice.bytes;
    batch.peer = notice.peer.to_string();
    batch.dir = Some(match batch.dir.take() {
        None => dir,
        Some(previous) => common_ancestor(&previous, &dir),
    });
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(0)
}

/// The deepest folder two paths share, so a drop that spread over subfolders
/// still opens at the folder it came from.
fn common_ancestor(a: &str, b: &str) -> String {
    let mut shared = std::path::PathBuf::new();
    for (left, right) in Path::new(a).components().zip(Path::new(b).components()) {
        if left != right {
            break;
        }
        shared.push(left);
    }
    if shared.as_os_str().is_empty() {
        a.to_string()
    } else {
        shared.to_string_lossy().into_owned()
    }
}

/// The nearest folder at or above `path`. The folder remembered for an upload
/// can be gone later — the visitor's subfolder was deleted, or the share was
/// removed — and walking up beats failing with an error the page cannot explain.
fn nearest_folder(path: &Path) -> Option<std::path::PathBuf> {
    let mut candidate = path.to_path_buf();
    for _ in 0..64 {
        if candidate.is_dir() {
            return Some(candidate);
        }
        if !candidate.pop() {
            break;
        }
    }
    None
}

#[tauri::command]
pub fn open_folder_command(path: String) -> Result<(), String> {
    let Some(folder) = nearest_folder(Path::new(&path)) else {
        return Err("folder_gone".into());
    };
    if open_folder(&folder) {
        Ok(())
    } else {
        Err("open_failed".into())
    }
}

pub(crate) fn append_log(app: &AppHandle, message: &str) {
    if let Ok(dir) = app.path().app_log_dir() {
        let line = format!(
            "{} {message}\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
        );
        crate::server::append_line(
            &dir.join("lantransport.log"),
            &line,
            crate::server::LOG_LIMIT,
        );
    }
}

fn record_start_error(app: &AppHandle, code: &str, notify_user: bool) {
    let state = app.state::<AppState>();
    *state
        .last_error
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = Some(code.to_string());
    append_log(app, &format!("start failed code={code}"));
    if notify_user {
        let locale = state.ui_locale();
        let reason = i18n::t(&locale, &format!("error.{code}"));
        let body = i18n::t_args(&locale, "notify.start_failed", &[("reason", reason)]);
        if let Err(error) = app
            .notification()
            .builder()
            .title("LanTransport")
            .body(body)
            .show()
        {
            append_log(app, &format!("notification failed: {error}"));
        }
    }
    notify(app);
}

fn start_server(
    app: &AppHandle,
    shares: &[String],
    port: u16,
    allow_lan: bool,
    password: Option<String>,
    upload: UploadConfig,
) -> Result<Snapshot, String> {
    if !(1..=65535).contains(&port) {
        return Err("invalid_port".into());
    }
    let state = app.state::<AppState>();
    {
        let mut current = state
            .server
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(mut server) = current.take() {
            server.stop();
        }
    }

    let paths: Vec<std::path::PathBuf> = shares.iter().map(std::path::PathBuf::from).collect();
    let log = app
        .path()
        .app_log_dir()
        .ok()
        .map(|dir| dir.join("lantransport.log"));
    let notify_app = app.clone();
    let on_urls_changed: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        let snapshot = state::snapshot(&notify_app);
        let _ = notify_app.emit("snapshot", snapshot);
        let _ = crate::tray::refresh(&notify_app);
    });
    let upload_app = app.clone();
    let on_upload: crate::server::UploadHook = Arc::new(move |notice| {
        let state = upload_app.state::<AppState>();
        let now = unix_now();
        if let Some(id) = notice.batch.clone() {
            // Part of a drop: nothing is shown until the page closes the batch,
            // so a 300-file folder does not become 300 rows and 300 toasts.
            let mut batches = state
                .batches
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            // A page that vanished mid-upload leaves its batch behind; ten
            // minutes of silence is enough to forget it.
            batches.retain(|_, batch| now.saturating_sub(batch.started) < 600);
            let batch = batches
                .entry(id)
                .or_insert_with(|| crate::state::PendingBatch {
                    started: now,
                    ..Default::default()
                });
            accumulate(batch, &notice);
            return;
        }
        {
            let mut recent = state
                .recent
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            recent.push_front(crate::state::RecentUpload {
                name: notice.name.clone(),
                bytes: notice.bytes,
                peer: notice.peer.to_string(),
                dir: notice.dir.to_string_lossy().into_owned(),
                at: now,
                files: 1,
            });
            while recent.len() > crate::state::RECENT_LIMIT {
                recent.pop_back();
            }
        }
        notify(&upload_app);
        let locale = state.ui_locale();
        let body = i18n::t_args(
            &locale,
            "notify.uploaded",
            &[
                ("name", notice.name.clone()),
                ("size", crate::server::listing::human_size(notice.bytes)),
                ("peer", notice.peer.to_string()),
            ],
        );
        if let Err(error) = upload_app
            .notification()
            .builder()
            .title("LanTransport")
            .body(body)
            .show()
        {
            append_log(&upload_app, &format!("upload notification failed: {error}"));
        }
        append_log(
            &upload_app,
            &format!("uploaded {} from {}", notice.name, notice.peer),
        );
    });

    let batch_app = app.clone();
    let on_batch_end: crate::server::BatchHook = Arc::new(move |end| {
        let state = batch_app.state::<AppState>();
        let pending = state
            .batches
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&end.id);
        // Nothing arrived for this batch, so there is nothing to report.
        let Some(pending) = pending else {
            return;
        };
        if pending.files == 0 {
            return;
        }
        let name = end.label.clone().unwrap_or_default();
        let dir = pending.dir.clone().unwrap_or_default();
        {
            let mut recent = state
                .recent
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            recent.push_front(crate::state::RecentUpload {
                name: name.clone(),
                bytes: pending.bytes,
                peer: pending.peer.clone(),
                dir: dir.clone(),
                at: unix_now(),
                files: pending.files,
            });
            while recent.len() > crate::state::RECENT_LIMIT {
                recent.pop_back();
            }
        }
        notify(&batch_app);
        let locale = state.ui_locale();
        let size = crate::server::listing::human_size(pending.bytes);
        let files = pending.files.to_string();
        let body = if name.is_empty() {
            i18n::t_args(
                &locale,
                "notify.uploaded_many",
                &[
                    ("files", files.clone()),
                    ("size", size.clone()),
                    ("peer", pending.peer.clone()),
                ],
            )
        } else {
            i18n::t_args(
                &locale,
                "notify.uploaded_folder",
                &[
                    ("name", name.clone()),
                    ("files", files.clone()),
                    ("size", size.clone()),
                    ("peer", pending.peer.clone()),
                ],
            )
        };
        if let Err(error) = batch_app
            .notification()
            .builder()
            .title("LanTransport")
            .body(body)
            .show()
        {
            append_log(&batch_app, &format!("upload notification failed: {error}"));
        }
        append_log(
            &batch_app,
            &format!(
                "uploaded folder={name:?} files={} bytes={} dir={dir} from {}",
                pending.files, pending.bytes, pending.peer
            ),
        );
    });

    let handle = ServerHandle::start(
        &paths,
        port,
        allow_lan,
        password,
        upload,
        log,
        Some(crate::server::Hooks {
            on_urls_changed: Some(on_urls_changed),
            on_upload: Some(on_upload),
            on_batch_end: Some(on_batch_end),
        }),
    )
    .map_err(|error| error_code(&error))?;

    append_log(
        app,
        &format!(
            "server started port={port} lan={allow_lan} folders={}",
            paths.len()
        ),
    );
    *state
        .server
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = Some(handle);
    *state
        .last_error
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = None;
    notify(app);
    Ok(state::snapshot(app))
}

fn stop_server(app: &AppHandle) -> Snapshot {
    let state = app.state::<AppState>();
    append_log(app, "server stopped");
    {
        let mut current = state
            .server
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(mut server) = current.take() {
            server.stop();
        }
    }
    *state
        .last_error
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = None;
    notify(app);
    state::snapshot(app)
}

pub fn shutdown(app: &AppHandle) {
    let state = app.state::<AppState>();
    let mut shutting_down = state
        .shutting_down
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if *shutting_down {
        return;
    }
    *shutting_down = true;
    drop(shutting_down);
    let mut current = state
        .server
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if let Some(mut server) = current.take() {
        server.stop();
    }
}

#[tauri::command]
pub fn get_snapshot(app: AppHandle) -> Snapshot {
    state::snapshot(&app)
}

#[tauri::command]
pub fn start_sharing(
    app: AppHandle,
    state: State<'_, AppState>,
    shares: Vec<String>,
    port: u16,
    password: Option<String>,
) -> Result<Snapshot, String> {
    let password = password.filter(|value| !value.is_empty());
    if shares.is_empty() {
        return Err("no_folder".into());
    }
    if let Some(value) = password.as_ref()
        && value.chars().count() < MIN_PASSWORD_CHARS
    {
        return Err("short_password".into());
    }

    {
        let mut settings = state
            .settings
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        settings.shares = shares.clone();
        settings.port = port;
        settings.password = password.clone();
    }
    let settings = app
        .state::<AppState>()
        .settings
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    match start_server(
        &app,
        &shares,
        port,
        true,
        password,
        upload_config(&settings),
    ) {
        Ok(snapshot) => {
            let _ = settings.save(&app);
            Ok(snapshot)
        }
        Err(code) => {
            record_start_error(&app, &code, false);
            Err(code)
        }
    }
}

#[tauri::command]
pub fn stop_sharing(app: AppHandle) -> Snapshot {
    stop_server(&app)
}

#[tauri::command]
pub fn update_prefs(
    app: AppHandle,
    state: State<'_, AppState>,
    locale: Option<String>,
    auto_start: Option<bool>,
    minimize_to_tray: Option<bool>,
    theme: Option<String>,
    shares: Option<Vec<String>>,
) -> Result<Snapshot, String> {
    let settings = {
        let mut settings = state
            .settings
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(theme) = theme {
            settings.theme = crate::settings::normalize_theme(&theme);
        }
        if let Some(shares) = shares {
            settings.shares = normalize_shares(shares);
        }
        if let Some(locale) = locale {
            settings.locale = if locale == "auto" {
                "auto".into()
            } else {
                i18n::normalize(&locale).unwrap_or("en").to_string()
            };
        }
        if let Some(auto_start) = auto_start {
            settings.auto_start = auto_start;
        }
        if let Some(minimize) = minimize_to_tray {
            settings.minimize_to_tray = minimize;
        }
        settings.clone()
    };
    let _ = settings.save(&app);
    notify(&app);
    Ok(state::snapshot(&app))
}

#[tauri::command]
pub fn update_upload(
    app: AppHandle,
    state: State<'_, AppState>,
    allow_uploads: bool,
    max_upload_mb: u32,
    regenerate: Option<bool>,
) -> Snapshot {
    let settings = {
        let mut settings = state
            .settings
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        settings.allow_uploads = allow_uploads;
        settings.max_upload_mb = max_upload_mb.clamp(1, 1024 * 1024);
        if allow_uploads && (settings.upload_key.is_empty() || regenerate == Some(true)) {
            settings.upload_key = generate_upload_key();
        }
        settings.clone()
    };
    if let Some(server) = state
        .server
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .as_ref()
    {
        server.set_upload(upload_config(&settings));
    }
    let _ = settings.save(&app);
    notify(&app);
    state::snapshot(&app)
}

fn generate_upload_key() -> String {
    use std::hash::{BuildHasher, Hasher};
    use std::time::{SystemTime, UNIX_EPOCH};
    const ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    let mut key = String::with_capacity(10);
    for index in 0..10usize {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_usize(index);
        hasher.write_u128(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |duration| duration.as_nanos()),
        );
        key.push(ALPHABET[(hasher.finish() % ALPHABET.len() as u64) as usize] as char);
    }
    key
}

#[tauri::command]
pub fn check_port(port: u16) -> bool {
    crate::server::lan::port_available(port)
}

#[tauri::command]
pub fn update_password(
    app: AppHandle,
    state: State<'_, AppState>,
    password: Option<String>,
) -> Snapshot {
    let settings = {
        let mut settings = state
            .settings
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        settings.password = password.filter(|value| !value.is_empty());
        settings.clone()
    };
    // Push it to the live server too, so a password typed while sharing
    // takes effect immediately instead of after a restart.
    {
        let password = settings
            .password
            .clone()
            .filter(|value| value.chars().count() >= MIN_PASSWORD_CHARS);
        if let Some(server) = state
            .server
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_ref()
        {
            server.set_password(password);
        }
    }
    let _ = settings.save(&app);
    state::snapshot(&app)
}

#[cfg(test)]
mod share_tests {
    use super::{accumulate, common_ancestor, nearest_folder, normalize_shares};

    #[test]
    fn shares_are_trimmed_and_deduplicated() {
        let out = normalize_shares(vec![" /a ".into(), String::new(), "/a".into(), "/b".into()]);
        assert_eq!(out, vec!["/a".to_string(), "/b".to_string()]);
    }

    #[test]
    fn a_batch_counts_files_bytes_and_the_shared_folder() {
        let mut batch = crate::state::PendingBatch::default();
        let notice = |name: &str, dir: &str, bytes: u64| crate::server::UploadNotice {
            name: name.to_string(),
            bytes,
            peer: "192.168.1.7".parse().unwrap(),
            dir: std::path::PathBuf::from(dir),
            batch: Some("b1".to_string()),
        };
        accumulate(&mut batch, &notice("a.txt", "/s/trip/day1", 10));
        accumulate(&mut batch, &notice("b.txt", "/s/trip/day2", 5));
        assert_eq!(batch.files, 2);
        assert_eq!(batch.bytes, 15);
        assert_eq!(batch.peer, "192.168.1.7");
        assert_eq!(batch.dir.as_deref(), Some("/s/trip"));
    }

    #[test]
    fn a_batch_opens_at_the_folder_its_files_share() {
        assert_eq!(
            common_ancestor("/s/trip/day1", "/s/trip/day2"),
            "/s/trip".to_string()
        );
        assert_eq!(
            common_ancestor("/s/trip/day1", "/s/trip/day1"),
            "/s/trip/day1".to_string()
        );
        assert_eq!(
            common_ancestor("/s/trip", "/s/other"),
            "/s".to_string(),
            "a drop spread over two folders opens at their parent"
        );
    }

    #[test]
    fn a_deleted_upload_folder_falls_back_to_the_nearest_parent() {
        let root =
            std::env::temp_dir().join(format!("lantransport-nearest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("shared/gone")).unwrap();
        let folder = root.join("shared/gone");
        assert_eq!(
            nearest_folder(&folder.join("file.txt")),
            Some(folder.clone())
        );
        std::fs::remove_dir(&folder).unwrap();
        assert_eq!(
            nearest_folder(&folder.join("file.txt")),
            Some(root.join("shared")),
            "the parent is opened when the upload folder was deleted"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
