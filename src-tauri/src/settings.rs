use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Legacy single-folder field, migrated into `shares` on load.
    pub root: Option<String>,
    pub shares: Vec<String>,
    pub port: u16,
    pub password: Option<String>,
    pub remember_password: bool,
    pub auto_start: bool,
    pub minimize_to_tray: bool,
    pub allow_uploads: bool,
    pub upload_key: String,
    pub max_upload_mb: u32,
    pub locale: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            root: None,
            shares: Vec::new(),
            port: 8000,
            password: None,
            remember_password: false,
            auto_start: false,
            minimize_to_tray: true,
            allow_uploads: false,
            upload_key: String::new(),
            max_upload_mb: 4096,
            locale: "auto".into(),
        }
    }
}

impl Settings {
    pub fn file(app: &AppHandle) -> Option<PathBuf> {
        app.path()
            .app_config_dir()
            .ok()
            .map(|dir| dir.join("settings.json"))
    }

    pub fn load(app: &AppHandle) -> Self {
        let Some(path) = Self::file(app) else {
            return Self::default();
        };
        let mut settings: Self = fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        if settings.shares.is_empty()
            && let Some(root) = settings.root.take()
        {
            settings.shares.push(root);
        }
        settings.root = None;
        if settings.locale != "auto" {
            settings.locale = crate::server::i18n::normalize(&settings.locale)
                .unwrap_or("en")
                .to_string();
        }
        settings
    }

    pub fn save(&self, app: &AppHandle) -> Result<(), String> {
        let path = Self::file(app).ok_or_else(|| "no config directory".to_string())?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|error| error.to_string())?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|error| error.to_string())?;
        let temp = path.with_extension("json.tmp");
        fs::write(&temp, text).map_err(|error| error.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&temp, fs::Permissions::from_mode(0o600));
        }
        fs::rename(&temp, &path).map_err(|error| error.to_string())
    }
}
