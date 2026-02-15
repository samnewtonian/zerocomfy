use std::path::{Path, PathBuf};
use serde::Deserialize;
use anyhow::{Context, Result};

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub authority: AuthorityConfig,
    #[serde(default)]
    pub cache: CacheConfig,
    #[serde(default)]
    pub api: ApiConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthorityConfig {
    pub interface: String,
    pub prefix: String,
    pub address: String,
    pub zone: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CacheConfig {
    #[serde(default = "default_db_path")]
    pub db_path: PathBuf,
    #[serde(default = "default_stale_after")]
    pub stale_after_secs: u64,
    #[serde(default = "default_prune_after")]
    pub prune_after_secs: u64,
    /// Fix #6: separate maintenance interval from browse interval
    #[serde(default = "default_maintenance_interval")]
    pub maintenance_interval_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiConfig {
    #[serde(default = "default_listen")]
    pub listen: String,
}

fn default_db_path() -> PathBuf {
    PathBuf::from("/var/lib/subnet-authority/services.db")
}

fn default_stale_after() -> u64 {
    300
}

fn default_prune_after() -> u64 {
    3600
}

fn default_maintenance_interval() -> u64 {
    60
}

fn default_listen() -> String {
    "[::]:8053".to_string()
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            db_path: default_db_path(),
            stale_after_secs: default_stale_after(),
            prune_after_secs: default_prune_after(),
            maintenance_interval_secs: default_maintenance_interval(),
        }
    }
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            listen: default_listen(),
        }
    }
}

impl Config {
    /// Load configuration from a TOML file
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        let config: Config = toml::from_str(&contents)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))?;

        Ok(config)
    }
}
