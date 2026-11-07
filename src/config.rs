use anyhow::{Context, Result};
use dirs::home_dir;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AsrBackendType {
    Auto,
    Phonon,
    Remote,
}

impl Default for AsrBackendType {
    fn default() -> Self {
        Self::Auto
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub asr_backend: AsrBackendType,
    pub s1_mini_endpoint: String,
    pub s1_mini_model: String,
    pub styling: String,
    pub structure: String,
    pub context: String,
    pub server_port: u16,
    pub remote_stt_url: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            asr_backend: AsrBackendType::Auto,
            s1_mini_endpoint: "http://127.0.0.1:11434/api/generate".to_string(),
            s1_mini_model: "s1-mini".to_string(),
            styling: "semi-formal".to_string(),
            structure: "prose".to_string(),
            context: "general".to_string(),
            server_port: 8090,
            remote_stt_url: None,
        }
    }
}

impl Config {
    pub fn config_dir() -> PathBuf {
        home_dir()
            .map(|h| h.join(".lightstt"))
            .unwrap_or_else(|| PathBuf::from(".lightstt"))
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.json")
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path();
        if !path.exists() {
            let default_cfg = Self::default();
            let _ = default_cfg.save();
            return Ok(default_cfg);
        }

        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config from {:?}", path))?;
        let cfg: Self = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse JSON config from {:?}", path))?;
        Ok(cfg)
    }

    pub fn save(&self) -> Result<()> {
        let dir = Self::config_dir();
        if !dir.exists() {
            fs::create_dir_all(&dir)
                .with_context(|| format!("Failed to create config dir {:?}", dir))?;
        }
        let path = Self::config_path();
        let json = serde_json::to_string_pretty(self)?;
        fs::write(&path, json)
            .with_context(|| format!("Failed to write config to {:?}", path))?;
        Ok(())
    }
}
