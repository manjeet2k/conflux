use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::warn;

/// Version of the on-disk settings layout, bumped when a migration is needed.
/// Files written before the field existed load as version 0.
pub const SCHEMA_VERSION: u32 = 1;
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
    #[serde(default = "legacy_schema_version")]
    pub schema_version: u32,
    pub theme: ThemePreference,
    /// `None` = the OS Downloads folder.
    pub default_save_dir: Option<String>,
    pub connections_per_adapter: u32,
    pub chunk_size_mb: u32,
    pub notify_on_complete: bool,
    pub close_to_tray: bool,
    pub auto_aggregate_adapters: bool,
    /// Quiet update check shortly after start; it only notifies, never installs.
    #[serde(default = "default_true")]
    pub check_updates_on_start: bool,
    pub adapter_overrides: std::collections::HashMap<String, bool>,
}

fn default_true() -> bool {
    true
}

fn legacy_schema_version() -> u32 {
    0
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            theme: ThemePreference::System,
            default_save_dir: None,
            connections_per_adapter: 4,
            chunk_size_mb: 4,
            notify_on_complete: true,
            close_to_tray: true,
            auto_aggregate_adapters: true,
            check_updates_on_start: true,
            adapter_overrides: std::collections::HashMap::new(),
        }
    }
}

impl Settings {
    /// Clamps numeric fields into their supported ranges, normalizes an empty folder to `None`
    /// and migrates legacy `"<name>:<ip>"` adapter override keys to interface names.
    pub fn normalized(mut self) -> Self {
        // Migrations for older `schema_version`s would run here, oldest first.
        self.schema_version = SCHEMA_VERSION;
        self.adapter_overrides = crate::adapters::migrate_overrides(self.adapter_overrides);
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

    /// Builds the settings to store from an update sent by the UI. Adapter overrides are
    /// changed only through `set_adapter_enabled`, so the UI's possibly stale copy is ignored.
    pub fn apply_update(&self, update: Settings) -> Result<Settings, String> {
        let mut next = update.normalized();
        // If the save directory was changed by the user, verify it exists.
        // If it was untouched, require it to be absolute but do not fail if an external
        // drive is temporarily unplugged while updating other settings.
        if next.default_save_dir != self.default_save_dir {
            next.validate()?;
        } else if let Some(dir) = &next.default_save_dir {
            let path = Path::new(dir);
            if !path.is_absolute() {
                return Err(format!("Default folder must be an absolute path: {dir}"));
            }
        }
        next.adapter_overrides = self.adapter_overrides.clone();
        Ok(next)
    }

    pub fn chunk_size_bytes(&self) -> u64 {
        u64::from(self.chunk_size_mb) * 1024 * 1024
    }
}

/// Loads settings. A missing file yields defaults. An unparseable file is renamed to
/// `settings.json.corrupt-<timestamp>` (at most 3 kept), defaults are used, and the second
/// value is a notice for the UI. Never fatal.
pub fn load(path: &Path) -> (Settings, Option<String>) {
    match std::fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str::<Settings>(&text) {
            Ok(s) => (s.normalized(), None),
            Err(e) => {
                warn!("Invalid settings file, using defaults: {e}");
                let notice = crate::fileutil::quarantine_with_notice(path, "settings");
                (Settings::default(), Some(notice))
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Settings::default(), None),
        Err(e) => {
            warn!("Cannot read settings file, using defaults: {e}");
            (Settings::default(), None)
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
    fn load_migrates_legacy_adapter_override_keys() {
        let dir = std::env::temp_dir().join(format!("conflux-settings-{}", uuid::Uuid::new_v4()));
        let path = dir.join("settings.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            &path,
            br#"{"adapter_overrides":{"Wi-Fi:192.168.1.5":false,"Ethernet:fe80::1":true}}"#,
        )
        .unwrap();
        let (s, notice) = load(&path);
        assert!(notice.is_none());
        assert_eq!(s.adapter_overrides.len(), 2);
        assert_eq!(s.adapter_overrides.get("Wi-Fi"), Some(&false));
        assert_eq!(s.adapter_overrides.get("Ethernet"), Some(&true));
        std::fs::remove_dir_all(&dir).unwrap();
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
    fn apply_update_keeps_current_adapter_overrides() {
        let mut current = Settings::default();
        current.adapter_overrides.insert("Wi-Fi".into(), false);
        // The UI sends its startup copy, which predates the adapter toggle.
        let update = Settings {
            theme: ThemePreference::Dark,
            connections_per_adapter: 0,
            ..Settings::default()
        };
        let next = current.apply_update(update).unwrap();
        assert_eq!(next.theme, ThemePreference::Dark);
        assert_eq!(next.connections_per_adapter, 1);
        assert_eq!(next.adapter_overrides, current.adapter_overrides);

        let bad = Settings {
            default_save_dir: Some("relative".into()),
            ..Settings::default()
        };
        assert!(current.apply_update(bad).is_err());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = std::env::temp_dir().join(format!("conflux-settings-{}", uuid::Uuid::new_v4()));
        let path = dir.join("settings.json");
        assert_eq!(load(&path), (Settings::default(), None));
        let mut overrides = std::collections::HashMap::new();
        overrides.insert("eth0".into(), false);
        overrides.insert("wlan0".into(), true);
        let s = Settings {
            theme: ThemePreference::Light,
            chunk_size_mb: 8,
            adapter_overrides: overrides,
            ..Settings::default()
        };
        save(&path, &s).unwrap();
        assert_eq!(load(&path), (s, None));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn corrupt_file_is_backed_up_and_defaults_used() {
        let dir = std::env::temp_dir().join(format!("conflux-settings-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(&path, b"{garbage").unwrap();
        let (s, notice) = load(&path);
        assert_eq!(s, Settings::default());
        assert!(notice.unwrap().contains("settings.json.corrupt-"));
        assert!(!path.exists(), "bad file moved away");
        let backups: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|n| n.starts_with("settings.json.corrupt-"))
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(std::fs::read(dir.join(&backups[0])).unwrap(), b"{garbage");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn schema_version_defaults_and_round_trips() {
        assert_eq!(Settings::default().schema_version, SCHEMA_VERSION);
        assert_eq!(
            serde_json::to_value(Settings::default()).unwrap()["schema_version"],
            1
        );
        // A file from before the field existed deserializes as version 0 ...
        let old: Settings = serde_json::from_str(r#"{"theme":"dark"}"#).unwrap();
        assert_eq!(old.schema_version, 0);
        // ... and is stamped current once normalized (the migration point).
        assert_eq!(old.normalized().schema_version, SCHEMA_VERSION);

        let dir = std::env::temp_dir().join(format!("conflux-settings-{}", uuid::Uuid::new_v4()));
        let path = dir.join("settings.json");
        save(&path, &Settings::default()).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"schema_version\": 1"));
        assert_eq!(load(&path).0.schema_version, 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn apply_update_allows_unplugged_save_dir_if_unchanged() {
        let missing = if cfg!(windows) {
            "Z:\\NonExistentDrive\\Downloads"
        } else {
            "/tmp/non_existent_drive/downloads"
        };
        let current = Settings {
            default_save_dir: Some(missing.to_string()),
            theme: ThemePreference::Light,
            ..Settings::default()
        };

        // Updating theme with untouched save folder succeeds even if drive is unplugged
        let update = Settings {
            default_save_dir: Some(missing.to_string()),
            theme: ThemePreference::Dark,
            ..Settings::default()
        };
        let next = current
            .apply_update(update)
            .expect("must succeed when save dir unchanged");
        assert_eq!(next.theme, ThemePreference::Dark);
        assert_eq!(next.default_save_dir.as_deref(), Some(missing));

        // Explicitly changing to a different non-existent path fails validation
        let bad_update = Settings {
            default_save_dir: Some(format!("{missing}_other")),
            ..Settings::default()
        };
        assert!(current.apply_update(bad_update).is_err());
    }
}
