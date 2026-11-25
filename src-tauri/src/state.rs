use std::sync::Mutex;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::server::{Lang, ServerHandle};
use crate::settings::Settings;

pub const LANG_ZH: u8 = 0;
pub const LANG_EN: u8 = 1;
pub const LANG_AUTO: u8 = 2;

pub struct AppState {
    pub settings: Mutex<Settings>,
    pub server: Mutex<Option<ServerHandle>>,
    pub lang: Arc<AtomicU8>,
    pub last_error: Mutex<Option<String>>,
    pub shutting_down: Mutex<bool>,
}

impl AppState {
    pub fn new(app: &AppHandle) -> Self {
        let settings = Settings::load(app);
        let lang = Arc::new(AtomicU8::new(locale_to_u8(&settings.locale)));
        Self {
            settings: Mutex::new(settings),
            server: Mutex::new(None),
            lang,
            last_error: Mutex::new(None),
            shutting_down: Mutex::new(false),
        }
    }

    pub fn lang_value(&self) -> Lang {
        match self.lang.load(Ordering::Relaxed) {
            LANG_EN => Lang::En,
            LANG_AUTO => Lang::Zh,
            _ => Lang::Zh,
        }
    }
}

pub fn locale_to_u8(locale: &str) -> u8 {
    match locale {
        "en" => LANG_EN,
        "zh" => LANG_ZH,
        _ => LANG_AUTO,
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
}

pub fn snapshot(app: &AppHandle) -> Snapshot {
    let state = app.state::<AppState>();
    let settings = state.settings.lock().unwrap_or_else(|error| error.into_inner()).clone();
    let server = state.server.lock().unwrap_or_else(|error| error.into_inner());
    let (running, port, urls, started_at_ms) = match server.as_ref() {
        Some(server) => (
            true,
            Some(server.port),
            server.urls.clone(),
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
        last_error: state.last_error.lock().unwrap_or_else(|error| error.into_inner()).clone(),
    }
}
