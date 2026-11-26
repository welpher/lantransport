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

fn append_log(app: &AppHandle, message: &str) {
    if let Ok(dir) = app.path().app_log_dir() {
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("lantransport.log"))
        {
            use std::io::Write;
            let _ = writeln!(
                file,
                "{} {message}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
            );
        }
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
    let handle = ServerHandle::start(&paths, port, allow_lan, password, upload, log)
        .map_err(|error| error_code(&error))?;

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
) -> Result<Snapshot, String> {
    let settings = {
        let mut settings = state
            .settings
            .lock()
            .unwrap_or_else(|error| error.into_inner());
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
