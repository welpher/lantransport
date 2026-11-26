use std::collections::VecDeque;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::server::ServerHandle;
use crate::server::i18n;
use crate::settings::Settings;

/// One finished upload, kept in memory for the Activity tab.
#[derive(Serialize, Clone)]
pub struct RecentUpload {
    pub name: String,
    pub bytes: u64,
    pub peer: String,
    pub dir: String,
    pub at: u64,
}

pub const RECENT_LIMIT: usize = 50;

pub struct AppState {
    pub settings: Mutex<Settings>,
    pub recent: Mutex<VecDeque<RecentUpload>>,
    pub server: Mutex<Option<ServerHandle>>,
    pub last_error: Mutex<Option<String>>,
    pub shutting_down: Mutex<bool>,
}

impl AppState {
    pub fn new(app: &AppHandle) -> Self {
        Self {
            settings: Mutex::new(Settings::load(app)),
            recent: Mutex::new(VecDeque::new()),
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
