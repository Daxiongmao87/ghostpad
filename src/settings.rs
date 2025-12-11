use std::fs;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::llm::LlmSettings;
use crate::paths::AppPaths;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub autosave_interval_secs: u64,
    #[serde(default)]
    pub recent_files: Vec<String>,
    #[serde(default)]
    pub autosave_idle_only: bool,
    #[serde(default)]
    pub llm: LlmSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            autosave_interval_secs: 60,
            recent_files: Vec::new(),
            autosave_idle_only: false,
            llm: LlmSettings::default(),
        }
    }
}

impl Settings {
    pub fn load(paths: &AppPaths) -> Result<Self> {
        if let Ok(raw) = fs::read_to_string(&paths.config_file) {
            Ok(toml::from_str(&raw).unwrap_or_default())
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self, paths: &AppPaths) -> Result<()> {
        let toml = toml::to_string_pretty(self).context("Failed to serialize settings")?;
        fs::write(&paths.config_file, toml).context("Failed to write settings")
    }
}
