use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::warn;

pub const MIN_CONNECTIONS: u32 = 1;
pub const MAX_CONNECTIONS: u32 = 16;
pub const MIN_CHUNK_MB: u32 = 1;
pub const MAX_CHUNK_MB: u32 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

/// User preferences, stored as JSON in the app config dir. Unknown or missing fields fall
/// back to defaults so old or hand-edited files still load.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: ThemePreference,
    /// `None` = the OS Downloads folder.
    pub default_save_dir: Option<String>,
    pub connections_per_adapter: u32,
    pub chunk_size_mb: u32,
    pub notify_on_complete: bool,
    pub close_to_tray: bool,
    pub auto_aggregate_adapters: bool,
    pub adapter_overrides: std::collections::HashMap<String, bool>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemePreference::System,
            default_save_dir: None,
            connections_per_adapter: 4,
            chunk_size_mb: 4,
            notify_on_complete: true,
            close_to_tray: true,
            auto_aggregate_adapters: true,
            adapter_overrides: std::collections::HashMap::new(),
        }
    }
}

impl Settings {
    /// Clamps numeric fields into their supported ranges and normalizes an empty folder to `None`.
    pub fn normalized(mut self) -> Self {
        self.connections_per_adapter = self
            .connections_per_adapter
            .clamp(MIN_CONNECTIONS, MAX_CONNECTIONS);
        self.chunk_size_mb = self.chunk_size_mb.clamp(MIN_CHUNK_MB, MAX_CHUNK_MB);
        self.default_save_dir = self
            .default_save_dir
            .map(|d| d.trim().to_string())
            .filter(|d| !d.is_empty());
        self
    }

    /// Rejects a default folder that is not an existing absolute directory.
    pub fn validate(&self) -> Result<(), String> {
        if let Some(dir) = &self.default_save_dir {
            let path = Path::new(dir);
            if !path.is_absolute() {
                return Err(format!("Default folder must be an absolute path: {dir}"));
            }
            if !path.is_dir() {
                return Err(format!("Default folder does not exist: {dir}"));
            }
        }
        Ok(())
    }

    pub fn chunk_size_bytes(&self) -> u64 {
        u64::from(self.chunk_size_mb) * 1024 * 1024
    }
}

/// Loads settings; a missing or unreadable file yields defaults (logged, never fatal).
pub fn load(path: &Path) -> Settings {
    match std::fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str::<Settings>(&text) {
            Ok(s) => s.normalized(),
            Err(e) => {
                warn!(path = %path.display(), "Invalid settings file, using defaults: {e}");
                Settings::default()
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Settings::default(),
        Err(e) => {
            warn!(path = %path.display(), "Cannot read settings file, using defaults: {e}");
            Settings::default()
        }
    }
}

pub fn save(path: &Path, settings: &Settings) -> Result<(), String> {
    let json = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    write_atomic(path, &json).map_err(|e| format!("Failed to save settings: {e}"))
}

/// Writes `data` to a temp file next to `path`, syncs it, then renames it over `path`.
pub fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut tmp = PathBuf::from(path);
    tmp.as_mut_os_string().push(".tmp");
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_clamping() {
        let s = Settings {
            connections_per_adapter: 0,
            chunk_size_mb: 1000,
            default_save_dir: Some("   ".into()),
            ..Settings::default()
        }
        .normalized();
        assert_eq!(s.connections_per_adapter, 1);
        assert_eq!(s.chunk_size_mb, 64);
        assert_eq!(s.default_save_dir, None);
        assert_eq!(Settings::default().chunk_size_bytes(), 4 * 1024 * 1024);
    }

    #[test]
    fn serde_names_and_partial_files() {
        let json = serde_json::to_value(Settings::default()).unwrap();
        assert_eq!(json["theme"], "system");
        assert_eq!(json["connections_per_adapter"], 4);
        assert_eq!(json["chunk_size_mb"], 4);
        assert_eq!(json["notify_on_complete"], true);
        assert_eq!(json["close_to_tray"], true);
        assert_eq!(json["auto_aggregate_adapters"], true);
        assert!(json["default_save_dir"].is_null());

        let partial: Settings = serde_json::from_str(r#"{"theme":"dark"}"#).unwrap();
        assert_eq!(partial.theme, ThemePreference::Dark);
        assert_eq!(partial.connections_per_adapter, 4);
        assert!(partial.adapter_overrides.is_empty());

        let with_overrides: Settings =
            serde_json::from_str(r#"{"adapter_overrides":{"eth0":false,"wlan0":true}}"#).unwrap();
        assert_eq!(with_overrides.adapter_overrides.get("eth0"), Some(&false));
        assert_eq!(with_overrides.adapter_overrides.get("wlan0"), Some(&true));
    }

    #[test]
    fn validate_default_dir() {
        let mut s = Settings::default();
        assert!(s.validate().is_ok());
        s.default_save_dir = Some("relative".into());
        assert!(s.validate().is_err());
        s.default_save_dir = Some(std::env::temp_dir().to_string_lossy().to_string());
        assert!(s.validate().is_ok());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = std::env::temp_dir().join(format!("conflux-settings-{}", uuid::Uuid::new_v4()));
        let path = dir.join("settings.json");
        assert_eq!(load(&path), Settings::default());
        let mut overrides = std::collections::HashMap::new();
        overrides.insert("eth0:192.168.1.50".into(), false);
        overrides.insert("wlan0".into(), true);
        let s = Settings {
            theme: ThemePreference::Light,
            chunk_size_mb: 8,
            adapter_overrides: overrides,
            ..Settings::default()
        };
        save(&path, &s).unwrap();
        assert_eq!(load(&path), s);
        std::fs::write(&path, b"{garbage").unwrap();
        assert_eq!(load(&path), Settings::default());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
