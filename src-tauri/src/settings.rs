use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub root: Option<String>,
    pub port: u16,
    pub allow_lan: bool,
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
            port: 8000,
            allow_lan: true,
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
        app.path().app_config_dir().ok().map(|dir| dir.join("settings.json"))
    }

    pub fn load(app: &AppHandle) -> Self {
        let Some(path) = Self::file(app) else { return Self::default() };
        fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
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
