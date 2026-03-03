use crate::error::ApixError;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RegistryConfig {
    pub remote: String,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            remote: "https://github.com/apix-sh/vault.git".to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SourceConfig {
    pub remote: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub color: bool,
    pub pager: Option<String>,
    #[serde(default = "default_auto_update")]
    pub auto_update: bool,
    #[serde(default = "default_auto_update_ttl_seconds")]
    pub auto_update_ttl_seconds: u64,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub source: HashMap<String, SourceConfig>,
    pub registry: RegistryConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            color: true,
            pager: None,
            auto_update: default_auto_update(),
            auto_update_ttl_seconds: default_auto_update_ttl_seconds(),
            sources: vec![".local".to_string(), "core".to_string()],
            source: HashMap::new(),
            registry: RegistryConfig::default(),
        }
    }
}

fn default_auto_update() -> bool {
    true
}

fn default_auto_update_ttl_seconds() -> u64 {
    21_600
}

impl Config {
    pub fn apix_home() -> Result<PathBuf, ApixError> {
        if let Ok(value) = std::env::var("APIX_HOME") {
            return Ok(PathBuf::from(value));
        }
        let home = dirs::home_dir()
            .ok_or_else(|| ApixError::Config("Unable to resolve home directory".to_string()))?;
        Ok(home.join(".apix"))
    }

    pub fn load() -> Result<Self, ApixError> {
        let path = Self::apix_home()?.join("config.toml");
        if !path.exists() {
            return Ok(Self::default());
        }

        let raw = std::fs::read_to_string(path)?;
        toml::from_str(&raw).map_err(|err| ApixError::Config(format!("Invalid config.toml: {err}")))
    }

    pub fn save(&self) -> Result<(), ApixError> {
        let path = Self::apix_home()?.join("config.toml");
        let rendered = toml::to_string_pretty(self)
            .map_err(|err| ApixError::Config(format!("Failed to serialize config: {err}")))?;
        std::fs::write(path, rendered)?;
        Ok(())
    }

    pub fn registry_remote(&self) -> String {
        std::env::var("APIX_REGISTRY_URL").unwrap_or_else(|_| self.registry.remote.clone())
    }

    pub fn source_priority(&self) -> Vec<String> {
        if let Ok(value) = std::env::var("APIX_SOURCES") {
            let out: Vec<String> = value
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToString::to_string)
                .collect();
            if !out.is_empty() {
                return out;
            }
        }
        if self.sources.is_empty() {
            vec![".local".to_string(), "core".to_string()]
        } else {
            self.sources.clone()
        }
    }

    pub fn known_sources(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for s in self.source_priority() {
            if seen.insert(s.clone()) {
                out.push(s);
            }
        }
        for s in [".local".to_string(), "core".to_string()] {
            if seen.insert(s.clone()) {
                out.push(s);
            }
        }
        for s in self.source.keys() {
            if seen.insert(s.clone()) {
                out.push(s.clone());
            }
        }
        out
    }

    pub fn source_remote(&self, name: &str) -> Option<String> {
        if name == "core" {
            return Some(self.registry_remote());
        }
        self.source.get(name).map(|c| c.remote.clone())
    }

    pub fn validate_source_name(name: &str) -> Result<(), ApixError> {
        if name.trim().is_empty() {
            return Err(ApixError::Config("Source name cannot be empty".to_string()));
        }
        if name.contains('/') {
            return Err(ApixError::Config(
                "Source name cannot contain '/'".to_string(),
            ));
        }
        if name == ".local" || name == "core" {
            return Err(ApixError::Config(format!(
                "Source `{name}` is reserved and cannot be modified"
            )));
        }
        Ok(())
    }

    pub fn auto_update_enabled(&self) -> bool {
        if let Ok(v) = std::env::var("APIX_AUTO_UPDATE") {
            return parse_bool_env(&v).unwrap_or(self.auto_update);
        }
        self.auto_update
    }

    pub fn auto_update_ttl_seconds(&self) -> u64 {
        if let Ok(v) = std::env::var("APIX_AUTO_UPDATE_TTL_SECONDS") {
            return v.parse::<u64>().unwrap_or(self.auto_update_ttl_seconds);
        }
        self.auto_update_ttl_seconds
    }
}

fn parse_bool_env(v: &str) -> Option<bool> {
    match v.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

pub fn init() -> Result<(), ApixError> {
    let home = Config::apix_home()?;
    let vaults = home.join("vaults");
    std::fs::create_dir_all(&vaults)?;
    std::fs::create_dir_all(vaults.join(".local"))?;
    std::fs::create_dir_all(vaults.join("core"))?;

    let config_path = home.join("config.toml");
    if !config_path.exists() {
        let cfg = Config::default();
        let rendered = toml::to_string_pretty(&cfg)
            .map_err(|err| ApixError::Config(format!("Failed to serialize config: {err}")))?;
        std::fs::write(config_path, rendered)?;
    }

    migrate_legacy_local_layout()?;

    Ok(())
}

pub fn migrate_legacy_local_layout() -> Result<(), ApixError> {
    let cfg = Config::load().unwrap_or_default();
    let vaults = Config::apix_home()?.join("vaults");
    let local_root = vaults.join(".local");
    std::fs::create_dir_all(&local_root)?;

    let mut reserved: HashSet<String> = cfg.known_sources().into_iter().collect();
    reserved.insert(".local".to_string());
    reserved.insert("core".to_string());

    for entry in std::fs::read_dir(&vaults)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if reserved.contains(&name) {
            continue;
        }
        let path = entry.path();
        if !looks_like_legacy_namespace(&path)? {
            continue;
        }

        let target = local_root.join(&name);
        if target.exists() {
            continue;
        }
        std::fs::rename(path, target)?;
    }
    Ok(())
}

fn looks_like_legacy_namespace(path: &std::path::Path) -> Result<bool, ApixError> {
    for child in std::fs::read_dir(path)? {
        let child = child?;
        if !child.file_type()?.is_dir() {
            continue;
        }
        if child.path().join("_metadata.md").exists() {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_source_name_checks() {
        assert!(Config::validate_source_name("acme").is_ok());
        assert!(Config::validate_source_name("acme-dev").is_ok());

        assert!(Config::validate_source_name("").is_err());
        assert!(Config::validate_source_name("  ").is_err());
        assert!(Config::validate_source_name("acme/dev").is_err());
        assert!(Config::validate_source_name(".local").is_err());
        assert!(Config::validate_source_name("core").is_err());
    }

    #[test]
    fn parse_bool_env_works() {
        assert_eq!(parse_bool_env("1"), Some(true));
        assert_eq!(parse_bool_env("true"), Some(true));
        assert_eq!(parse_bool_env("YES"), Some(true));
        assert_eq!(parse_bool_env("On"), Some(true));

        assert_eq!(parse_bool_env("0"), Some(false));
        assert_eq!(parse_bool_env("false"), Some(false));
        assert_eq!(parse_bool_env("NO"), Some(false));
        assert_eq!(parse_bool_env("Off"), Some(false));

        assert_eq!(parse_bool_env("invalid"), None);
        assert_eq!(parse_bool_env(""), None);
    }

    #[test]
    fn config_defaults() {
        let cfg = Config::default();
        assert!(cfg.color);
        assert!(cfg.auto_update);
        assert_eq!(cfg.auto_update_ttl_seconds, 21600);
        assert_eq!(cfg.sources, vec![".local", "core"]);
        assert!(cfg.source.is_empty());
        assert_eq!(cfg.registry.remote, "https://github.com/apix-sh/vault.git");
    }

    #[test]
    fn source_remote_resolution() {
        let mut cfg = Config::default();
        cfg.source.insert(
            "acme".to_string(),
            SourceConfig {
                remote: "https://git.acme.com/vaults.git".to_string(),
            },
        );

        assert_eq!(
            cfg.source_remote("core").unwrap(),
            "https://github.com/apix-sh/vault.git"
        );
        assert_eq!(
            cfg.source_remote("acme").unwrap(),
            "https://git.acme.com/vaults.git"
        );
        assert!(cfg.source_remote("unknown").is_none());
    }
}
