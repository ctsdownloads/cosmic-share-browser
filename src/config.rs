use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    pub enabled: bool,
    pub shared_dir: String,
    pub service_name: String,
    #[serde(default = "default_read_only")]
    pub read_only: bool,
}

fn default_read_only() -> bool { true }

impl Default for Config {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| String::from("/home/user"));
        Self {
            enabled: false,
            shared_dir: format!("{}/Public", home),
            service_name: String::from("COSMIC-Share"),
            read_only: true,
        }
    }
}

impl Config {
    pub fn path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| String::from("/home/user"));
        PathBuf::from(home).join(".config/cosmic-share-browser/config.toml")
    }

    pub fn load() -> Self {
        let path = Self::path();
        if path.exists() {
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            toml::from_str(&content).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, toml::to_string_pretty(self).unwrap_or_default())
    }

    pub fn mtime() -> Option<SystemTime> {
        std::fs::metadata(Self::path()).ok()?.modified().ok()
    }

    pub fn service_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| String::from("/home/user"));
        PathBuf::from(home).join(".config/systemd/user/cosmic-share-daemon.service")
    }

    /// Path where the daemon writes its current port so the GUI can read it.
    /// Uses XDG_RUNTIME_DIR (/run/user/<uid>) — tmpfs, cleaned on logout.
    pub fn runtime_port_path() -> PathBuf {
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
            .unwrap_or_else(|_| String::from("/tmp"));
        PathBuf::from(runtime_dir).join("cosmic-share-daemon.port")
    }

    /// Read the daemon's current port from the runtime file, if it exists.
    pub fn read_daemon_port() -> Option<u16> {
        let path = Self::runtime_port_path();
        std::fs::read_to_string(&path)
            .ok()?
            .trim()
            .parse()
            .ok()
    }

    /// Persistent file that remembers the last firewall port we opened.
    /// Lives in ~/.config (survives reboot, unlike XDG_RUNTIME_DIR).
    fn last_port_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| String::from("/home/user"));
        PathBuf::from(home).join(".config/cosmic-share-browser/last_port")
    }

    pub fn read_last_port() -> Option<u16> {
        std::fs::read_to_string(Self::last_port_path())
            .ok()?
            .trim()
            .parse()
            .ok()
    }

    pub fn write_last_port(port: u16) {
        let path = Self::last_port_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(path, port.to_string()).ok();
    }

    pub fn clear_last_port() {
        std::fs::remove_file(Self::last_port_path()).ok();
    }
}
