use std::collections::VecDeque;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::server::ServerHandle;
use crate::server::i18n;
use crate::settings::Settings;

/// One finished upload, kept for the Activity tab across restarts.
#[derive(Serialize, Deserialize, Clone)]
pub struct RecentUpload {
    pub name: String,
    pub bytes: u64,
    pub peer: String,
    pub dir: String,
    pub at: u64,
    /// How many files arrived in one drop; 1 for a plain upload.
    #[serde(default = "one")]
    pub files: u32,
    /// The batch this row belongs to, so the row can grow while files arrive.
    #[serde(default)]
    pub batch: Option<String>,
}

fn one() -> u32 {
    1
}

/// Files of one drop that have arrived so far, keyed by the id the page sends.
/// The page closes the batch with `end=1` and the app then writes one entry.
#[derive(Default)]
pub struct PendingBatch {
    pub label: Option<String>,
    pub peer: String,
    pub files: u32,
    pub bytes: u64,
    pub dir: Option<String>,
    pub started: u64,
    /// What the page said it was about to send, for the first notification.
    pub expected_files: u32,
    pub expected_bytes: u64,
    /// A row in `recent` exists, so later files only update it.
    pub row: bool,
    /// The drop was a folder (the page sent a name), so it becomes one row;
    /// a multi-file pick gets one row per file.
    pub folder: bool,
    /// When the window was last told about this batch, in milliseconds.
    pub last_emit: u64,
}

pub const RECENT_LIMIT: usize = 50;

fn recent_file(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|dir| dir.join("recent.json"))
}

/// The Activity list survives a restart; it is a small JSON file next to the
/// settings. Anything unreadable just means an empty list, never an error on
/// start-up.
pub fn load_recent(app: &AppHandle) -> VecDeque<RecentUpload> {
    let Some(path) = recent_file(app) else {
        return VecDeque::new();
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return VecDeque::new();
    };
    let mut recent = parse_recent(&text);
    while recent.len() > RECENT_LIMIT {
        recent.pop_back();
    }
    recent
}

fn parse_recent(text: &str) -> VecDeque<RecentUpload> {
    serde_json::from_str(text).unwrap_or_default()
}

pub fn save_recent(app: &AppHandle, recent: &VecDeque<RecentUpload>) {
    let Some(path) = recent_file(app) else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let Ok(text) = serde_json::to_string(recent) else {
        return;
    };
    let temp = path.with_extension("json.tmp");
    if std::fs::write(&temp, text).is_ok() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o600));
        }
        let _ = std::fs::rename(&temp, &path);
    }
}

pub struct AppState {
    pub settings: Mutex<Settings>,
    pub recent: Mutex<VecDeque<RecentUpload>>,
    pub batches: Mutex<std::collections::HashMap<String, PendingBatch>>,
    pub server: Mutex<Option<ServerHandle>>,
    pub last_error: Mutex<Option<String>>,
    pub shutting_down: Mutex<bool>,
}

impl AppState {
    pub fn new(app: &AppHandle) -> Self {
        Self {
            settings: Mutex::new(Settings::load(app)),
            recent: Mutex::new(load_recent(app)),
            batches: Mutex::new(std::collections::HashMap::new()),
            server: Mutex::new(None),
            last_error: Mutex::new(None),
            shutting_down: Mutex::new(false),
        }
    }

    pub fn ui_locale(&self) -> String {
        let locale = self
            .settings
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .locale
            .clone();
        if locale == "auto" {
            std::env::var("LANG")
                .ok()
                .and_then(|value| i18n::normalize(&value))
                .unwrap_or("en")
                .to_string()
        } else {
            i18n::normalize(&locale).unwrap_or("en").to_string()
        }
    }
}

#[derive(Serialize, Clone)]
pub struct Snapshot {
    pub version: String,
    pub platform: String,
    pub settings: Settings,
    pub running: bool,
    pub port: Option<u16>,
    pub urls: Vec<String>,
    pub started_at_ms: Option<u64>,
    pub last_error: Option<String>,
    pub recent: Vec<RecentUpload>,
}

pub fn snapshot(app: &AppHandle) -> Snapshot {
    let state = app.state::<AppState>();
    let settings = state
        .settings
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    let server = state
        .server
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (running, port, urls, started_at_ms) = match server.as_ref() {
        Some(server) => (
            true,
            Some(server.port),
            server
                .urls
                .read()
                .unwrap_or_else(|error| error.into_inner())
                .clone(),
            server
                .started_at
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|duration| duration.as_millis() as u64),
        ),
        None => (false, None, Vec::new(), None),
    };
    Snapshot {
        version: app.package_info().version.to_string(),
        platform: std::env::consts::OS.to_string(),
        settings,
        running,
        port,
        urls,
        started_at_ms,
        last_error: state
            .last_error
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone(),
        recent: state
            .recent
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .cloned()
            .collect(),
    }
}

#[cfg(test)]
mod recent_tests {
    use super::parse_recent;

    #[test]
    fn entries_without_the_new_fields_still_load() {
        // A file written by 0.3.2 had neither `files` nor `batch`.
        let text = r#"[{"name":"a.txt","bytes":3,"peer":"192.168.1.7","dir":"/s","at":1}]"#;
        let recent = parse_recent(text);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].name, "a.txt");
        assert_eq!(recent[0].files, 1, "a single file is the default");
        assert!(recent[0].batch.is_none());
    }

    #[test]
    fn a_broken_file_is_an_empty_list() {
        assert!(parse_recent("not json").is_empty());
        assert!(parse_recent("[]").is_empty());
    }
}
