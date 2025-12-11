use std::fs;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::paths::AppPaths;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowState {
    pub width: i32,
    pub height: i32,
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            width: 1024,
            height: 720,
        }
    }
}

impl WindowState {
    pub fn load(paths: &AppPaths) -> Result<Self> {
        if let Ok(raw) = fs::read_to_string(&paths.state_file) {
            let parsed: Self = serde_json::from_str(&raw).context("Invalid state.json format")?;
            Ok(parsed)
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self, paths: &AppPaths) -> Result<()> {
        let data = serde_json::to_string_pretty(self).context("Serialize window state")?;
        fs::write(&paths.state_file, data).context("Write window state")
    }
}
