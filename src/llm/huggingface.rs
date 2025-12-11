use std::path::{Path, PathBuf};
use anyhow::{anyhow, Context, Result};

/// Parse a Hugging Face model reference like:
/// "mradermacher/Luau-Qwen3-4B-FIM-v0.1-i1-GGUF:Q4_K_M"
/// into (repo, filename)
#[derive(Debug, Clone)]
pub struct HuggingFaceModel {
    pub repo: String,
    pub file: String,
}

impl HuggingFaceModel {
    pub fn parse(reference: &str) -> Result<Self> {
        // Format: owner/repo:file or owner/repo/file
        let parts: Vec<&str> = reference.split(':').collect();

        if parts.len() == 2 {
            // Format: owner/repo:file
            let repo = parts[0].to_string();
            let file = parts[1].to_string();

            if !repo.contains('/') {
                return Err(anyhow!("Invalid HF format: repo must be 'owner/repo'"));
            }

            Ok(Self { repo, file })
        } else if parts.len() == 1 && reference.contains('/') {
            // Could be owner/repo/file format
            let path_parts: Vec<&str> = reference.split('/').collect();
            if path_parts.len() >= 3 {
                let repo = format!("{}/{}", path_parts[0], path_parts[1]);
                let file = path_parts[2..].join("/");
                Ok(Self { repo, file })
            } else {
                Err(anyhow!("Invalid HF format: expected 'owner/repo:file' or 'owner/repo/file'"))
            }
        } else {
            Err(anyhow!("Invalid HF format: expected 'owner/repo:file'"))
        }
    }

    pub fn download_url(&self) -> String {
        format!(
            "https://huggingface.co/{}/resolve/main/{}",
            self.repo, self.file
        )
    }

    pub fn filename(&self) -> String {
        self.file.split('/').last().unwrap_or(&self.file).to_string()
    }
}

pub struct ModelDownloader {
    models_dir: PathBuf,
}

impl ModelDownloader {
    pub fn new(models_dir: PathBuf) -> Self {
        Self { models_dir }
    }

    /// Download a model from Hugging Face to the models directory
    /// Returns the path to the downloaded file
    pub fn download(&self, model: &HuggingFaceModel) -> Result<PathBuf> {
        std::fs::create_dir_all(&self.models_dir)
            .context("Failed to create models directory")?;

        let filename = model.filename();
        let output_path = self.models_dir.join(&filename);

        // If already downloaded, return the path
        if output_path.exists() {
            log::info!("Model already downloaded: {}", output_path.display());
            return Ok(output_path);
        }

        let url = model.download_url();
        log::info!("Downloading model from: {}", url);

        // Use ureq for synchronous HTTP download
        let response = ureq::get(&url)
            .call()
            .map_err(|e| anyhow!("Failed to download model: {}", e))?;

        let total_size = response
            .header("content-length")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        log::info!("Download size: {} bytes", total_size);

        // Write to temp file first, then rename atomically
        let temp_path = output_path.with_extension("tmp");
        let mut file = std::fs::File::create(&temp_path)
            .context("Failed to create temp file")?;

        let mut reader = response.into_reader();
        std::io::copy(&mut reader, &mut file)
            .context("Failed to write model file")?;

        // Atomic rename
        std::fs::rename(&temp_path, &output_path)
            .context("Failed to rename downloaded model")?;

        log::info!("Model downloaded to: {}", output_path.display());
        Ok(output_path)
    }

    /// Check if a model is already downloaded
    pub fn is_downloaded(&self, model: &HuggingFaceModel) -> bool {
        let filename = model.filename();
        self.models_dir.join(&filename).exists()
    }

    /// Get path to a model if it's downloaded
    pub fn get_path(&self, model: &HuggingFaceModel) -> Option<PathBuf> {
        let filename = model.filename();
        let path = self.models_dir.join(&filename);
        if path.exists() {
            Some(path)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hf_model() {
        let model = HuggingFaceModel::parse("owner/repo:file.gguf").unwrap();
        assert_eq!(model.repo, "owner/repo");
        assert_eq!(model.file, "file.gguf");
        assert_eq!(model.filename(), "file.gguf");
    }

    #[test]
    fn test_parse_hf_model_with_path() {
        let model = HuggingFaceModel::parse("owner/repo/path/to/file.gguf").unwrap();
        assert_eq!(model.repo, "owner/repo");
        assert_eq!(model.file, "path/to/file.gguf");
        assert_eq!(model.filename(), "file.gguf");
    }

    #[test]
    fn test_download_url() {
        let model = HuggingFaceModel::parse("mradermacher/Luau-Qwen3-4B:Q4_K_M.gguf").unwrap();
        assert_eq!(
            model.download_url(),
            "https://huggingface.co/mradermacher/Luau-Qwen3-4B/resolve/main/Q4_K_M.gguf"
        );
    }
}
