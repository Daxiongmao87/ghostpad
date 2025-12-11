use std::fs;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::paths::AppPaths;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub autosave_interval_secs: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            autosave_interval_secs: 60,
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

    #[allow(dead_code)]
    pub fn save(&self, paths: &AppPaths) -> Result<()> {
        let toml = toml::to_string_pretty(self).context("Failed to serialize settings")?;
        fs::write(&paths.config_file, toml).context("Failed to write settings")
    }
}
