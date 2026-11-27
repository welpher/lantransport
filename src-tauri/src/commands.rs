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

#[tauri::command]
pub fn open_folder_command(path: String) -> Result<(), String> {
    if open_folder(std::path::Path::new(&path)) {
        Ok(())
    } else {
        Err("not a folder".into())
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
                at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|value| value.as_secs())
                    .unwrap_or(0),
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
    use super::normalize_shares;

    #[test]
    fn shares_are_trimmed_and_deduplicated() {
        let out = normalize_shares(vec![" /a ".into(), String::new(), "/a".into(), "/b".into()]);
        assert_eq!(out, vec!["/a".to_string(), "/b".to_string()]);
    }
}
