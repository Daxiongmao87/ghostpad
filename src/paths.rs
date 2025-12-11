use anyhow::{Context, Result};
use directories::ProjectDirs;
use std::path::{Path, PathBuf};

pub struct AppPaths {
    pub config_file: PathBuf,
    pub state_file: PathBuf,
    pub autosave_dir: PathBuf,
    pub models_dir: PathBuf,
}

impl AppPaths {
    pub fn initialize() -> Result<Self> {
        let dirs = ProjectDirs::from("com", "Ghostpad", "ghostpad")
            .context("Unable to determine XDG directories")?;
        let config_dir = dirs.config_dir().to_path_buf();
        let data_dir = dirs.data_dir().to_path_buf();
        let state_dir = dirs
            .state_dir()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| data_dir.clone());
        let config_file = config_dir.join("config.toml");
        let state_file = state_dir.join("state.json");
        std::fs::create_dir_all(&config_dir).context("Failed to create config directory")?;
        std::fs::create_dir_all(&state_dir).context("Failed to create state directory")?;
        let autosave_dir = state_dir.join("autosave");
        std::fs::create_dir_all(&autosave_dir).context("Failed to create autosave directory")?;
        let models_dir = data_dir.join("models");
        std::fs::create_dir_all(&models_dir).context("Failed to create models directory")?;
        Ok(Self {
            config_file,
            state_file,
            autosave_dir,
            models_dir,
        })
    }
}
