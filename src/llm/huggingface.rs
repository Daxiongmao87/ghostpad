use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::from_reader;
use sha2::{Digest, Sha256};

/// Parse a Hugging Face model reference like:
/// "mradermacher/Luau-Qwen3-4B-FIM-v0.1-i1-GGUF:Q4_K_M"
/// into (repo, filename)
#[derive(Debug, Clone)]
pub struct HuggingFaceModel {
    pub repo: String,
    pub revision: String,
    pub file: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadPhase {
    Preparing,
    VerifyingExisting,
    Downloading,
    Finished,
}

#[derive(Clone, Copy, Debug)]
pub struct DownloadProgress {
    pub phase: DownloadPhase,
    pub downloaded: u64,
    pub total: Option<u64>,
}

impl HuggingFaceModel {
    pub fn parse(reference: &str) -> Result<Self> {
        if reference.trim().is_empty() {
            return Err(anyhow!("Empty Hugging Face reference"));
        }

        // allow formats:
        // repo[:file]
        // repo@revision[:file]
        // repo/path/to/file
        let (left, right_opt) = reference
            .split_once(':')
            .map(|(repo_part, file_part)| (repo_part, Some(file_part)))
            .unwrap_or((reference, None));

        let (repo_with_owner, revision) = if let Some((repo, rev)) = left.split_once('@') {
            (repo, rev.to_string())
        } else {
            (left, "main".to_string())
        };

        let repo_parts: Vec<&str> = repo_with_owner.split('/').collect();
        if repo_parts.len() < 2 {
            return Err(anyhow!("Invalid HF repo format: expected 'owner/repo'"));
        }
        let repo = format!("{}/{}", repo_parts[0], repo_parts[1]);

        // Determine file path either from explicit :file, or extra path segments.
        let mut file_candidate: Option<String> = right_opt
            .map(|part| part.trim_matches('/').to_string())
            .filter(|s| !s.is_empty());

        if file_candidate.is_none() && repo_parts.len() > 2 {
            file_candidate = Some(repo_parts[2..].join("/"));
        }

        let file = file_candidate.ok_or_else(|| {
            anyhow!(
                "Missing filename; provide 'owner/repo:relative/path/to/file.gguf'"
            )
        })?;

        Ok(Self {
            repo,
            revision,
            file,
        })
    }

    pub fn download_url(&self) -> String {
        format!(
            "https://huggingface.co/{}/resolve/{}/{}?download=1",
            self.repo, self.revision, self.file
        )
    }

    pub fn filename(&self) -> String {
        self.file.split('/').last().unwrap_or(&self.file).to_string()
    }

    fn needs_filename_resolution(&self) -> bool {
        !self.file.contains('/') && !self.file.contains('.')
    }

    fn materialize_filename(&mut self) -> Result<()> {
        if !self.needs_filename_resolution() {
            return Ok(());
        }

        let alias = self.file.clone();
        let resolved = resolve_hf_alias(&self.repo, &alias)?;
        log::info!(
            "Resolved Hugging Face alias '{}' -> '{}' for repo {}",
            alias, resolved, self.repo
        );
        self.file = resolved;
        Ok(())
    }
}

#[derive(Clone, Debug)]
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
        self.download_with_progress(model, |_| {})
    }

    pub fn download_with_progress<F>(
        &self,
        model: &HuggingFaceModel,
        mut progress: F,
    ) -> Result<PathBuf>
    where
        F: FnMut(DownloadProgress),
    {
        let mut resolved = model.clone();
        resolved.materialize_filename()?;

        progress(DownloadProgress {
            phase: DownloadPhase::Preparing,
            downloaded: 0,
            total: None,
        });

        fs::create_dir_all(&self.models_dir)
            .context("Failed to create models directory")?;

        let filename = resolved.filename();
        let output_path = self.models_dir.join(&filename);
        let metadata_path = self.metadata_path(&filename);

        if output_path.exists() {
            match self.verify_existing_file_with_progress(
                &output_path,
                metadata_path.as_path(),
                &mut progress,
            ) {
                Ok(true) => {
                    let file_size = fs::metadata(&output_path).map(|m| m.len()).unwrap_or(0);
                    progress(DownloadProgress {
                        phase: DownloadPhase::Finished,
                        downloaded: file_size,
                        total: Some(file_size),
                    });
                    log::info!(
                        "Model already downloaded with matching hash: {}",
                        output_path.display()
                    );
                    return Ok(output_path);
                }
                Ok(false) | Err(_) => {
                    log::warn!(
                        "Existing model {} failed verification, re-downloading",
                        output_path.display()
                    );
                    let _ = fs::remove_file(&output_path);
                    let _ = fs::remove_file(&metadata_path);
                }
            }
        }

        let url = resolved.download_url();
        log::info!("Downloading model from: {}", url);

        // Use ureq for synchronous HTTP download
        let response = ureq::get(&url)
            .call()
            .map_err(|e| anyhow!("Failed to download model: {}", e))?;

        let expected_hash = response
            .header("x-linked-etag")
            .or_else(|| response.header("x-xet-hash"))
            .map(|value| value.trim_matches('"').to_lowercase());

        let total_size = response
            .header("content-length")
            .and_then(|s| s.parse::<u64>().ok());

        log::info!(
            "Download size: {}",
            total_size
                .map(|sz| format!("{} bytes", sz))
                .unwrap_or_else(|| "unknown".into())
        );

        // Write to temp file first, then rename atomically
        let temp_path = output_path.with_extension("tmp");
        let mut file = File::create(&temp_path)
            .context("Failed to create temp file")?;

        let mut reader = response.into_reader();
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 1024 * 64];
        let mut downloaded_bytes: u64 = 0;

        progress(DownloadProgress {
            phase: DownloadPhase::Downloading,
            downloaded: 0,
            total: total_size,
        });

        loop {
            let read = reader
                .read(&mut buffer)
                .context("Failed to read model bytes")?;
            if read == 0 {
                break;
            }
            file.write_all(&buffer[..read])
                .context("Failed to write model file")?;
            hasher.update(&buffer[..read]);
            downloaded_bytes += read as u64;
            progress(DownloadProgress {
                phase: DownloadPhase::Downloading,
                downloaded: downloaded_bytes,
                total: total_size,
            });
        }
        let hash_hex = format!("{:x}", hasher.finalize());

        if let Some(ref expected) = expected_hash {
            if expected != &hash_hex {
                let _ = fs::remove_file(&temp_path);
                anyhow::bail!(
                    "Hash mismatch: expected {}, got {}",
                    expected,
                    hash_hex
                );
            }
        }

        // Atomic rename
        fs::rename(&temp_path, &output_path)
            .context("Failed to rename downloaded model")?;

        self.write_metadata(&metadata_path, &hash_hex, expected_hash.as_deref())?;

        let final_total = total_size.or(Some(downloaded_bytes));
        progress(DownloadProgress {
            phase: DownloadPhase::Finished,
            downloaded: downloaded_bytes,
            total: final_total,
        });

        log::info!("Model downloaded to: {}", output_path.display());
        Ok(output_path)
    }

    /// Check if a model is already downloaded
    pub fn is_downloaded(&self, model: &HuggingFaceModel) -> bool {
        let mut resolved = model.clone();
        if let Err(err) = resolved.materialize_filename() {
            log::warn!(
                "Failed to resolve Hugging Face alias for {}: {}",
                model.repo, err
            );
            return false;
        }

        let filename = resolved.filename();
        let path = self.models_dir.join(&filename);
        let metadata_path = self.metadata_path(&filename);
        match self.verify_existing_file(&path, metadata_path.as_path()) {
            Ok(true) => true,
            Ok(false) | Err(_) => false,
        }
    }

    /// Get path to a model if it's downloaded
    pub fn get_path(&self, model: &HuggingFaceModel) -> Option<PathBuf> {
        let mut resolved = model.clone();
        if let Err(err) = resolved.materialize_filename() {
            log::warn!(
                "Failed to resolve Hugging Face alias for {}: {}",
                model.repo, err
            );
            return None;
        }

        let filename = resolved.filename();
        let path = self.models_dir.join(&filename);
        let metadata_path = self.metadata_path(&filename);
        match self.verify_existing_file(&path, metadata_path.as_path()) {
            Ok(true) => Some(path),
            Ok(false) | Err(_) => None,
        }
    }
}

#[derive(Deserialize)]
struct ModelInfo {
    siblings: Vec<ModelSibling>,
}

#[derive(Deserialize)]
struct ModelSibling {
    rfilename: String,
}

fn resolve_hf_alias(repo: &str, alias: &str) -> Result<String> {
    let url = format!("https://huggingface.co/api/models/{}", repo);
    let response = ureq::get(&url)
        .call()
        .map_err(|e| anyhow!("Failed to resolve alias '{}': {}", alias, e))?;

    let info: ModelInfo = from_reader(response.into_reader())
        .map_err(|e| anyhow!("Failed to parse model metadata for {}: {}", repo, e))?;

    let alias_lower = alias.to_lowercase();

    let mut candidates: Vec<String> = info
        .siblings
        .iter()
        .map(|s| s.rfilename.clone())
        .filter(|name| name.to_lowercase().contains(&alias_lower))
        .filter(|name| name.to_lowercase().ends_with(".gguf"))
        .collect();

    if candidates.is_empty() {
        return Err(anyhow!(
            "Could not find a GGUF file containing '{}' in repo {}",
            alias, repo
        ));
    }

    // Prefer exact suffix match, otherwise pick the shortest.
    let suffix = format!("{}{}", alias_lower, ".gguf");
    if let Some(exact) = candidates
        .iter()
        .find(|name| name.to_lowercase().ends_with(&suffix))
    {
        return Ok(exact.clone());
    }

    candidates.sort_by_key(|name| name.len());
    Ok(candidates[0].clone())
}

#[derive(Debug, Serialize, Deserialize)]
struct DownloadMetadata {
    sha256: String,
    etag: Option<String>,
}

impl ModelDownloader {
    fn metadata_path(&self, filename: &str) -> PathBuf {
        self.models_dir
            .join(format!("{}.meta.json", filename))
    }

    fn write_metadata(
        &self,
        metadata_path: &Path,
        sha256_hex: &str,
        etag: Option<&str>,
    ) -> Result<()> {
        let metadata = DownloadMetadata {
            sha256: sha256_hex.to_string(),
            etag: etag.map(|s| s.to_string()),
        };
        let json = serde_json::to_string_pretty(&metadata)?;
        fs::write(metadata_path, json)
            .with_context(|| format!("Failed to write metadata: {}", metadata_path.display()))
    }

    fn verify_existing_file(&self, path: &Path, metadata_path: &Path) -> Result<bool> {
        self.verify_existing_file_internal(path, metadata_path, None)
    }

    fn verify_existing_file_with_progress(
        &self,
        path: &Path,
        metadata_path: &Path,
        progress: &mut dyn FnMut(DownloadProgress),
    ) -> Result<bool> {
        self.verify_existing_file_internal(path, metadata_path, Some(progress))
    }

    fn verify_existing_file_internal(
        &self,
        path: &Path,
        metadata_path: &Path,
        progress: Option<&mut dyn FnMut(DownloadProgress)>,
    ) -> Result<bool> {
        if !path.exists() || !metadata_path.exists() {
            return Ok(false);
        }
        let metadata_bytes = fs::read(metadata_path).with_context(|| {
            format!("Failed to read metadata file: {}", metadata_path.display())
        })?;
        let metadata: DownloadMetadata =
            serde_json::from_slice(&metadata_bytes).context("Invalid metadata json")?;
        let computed = match progress {
            Some(cb) => self.compute_sha256_with_progress(path, Some(cb))?,
            None => self.compute_sha256_with_progress(path, None)?,
        };
        Ok(computed == metadata.sha256)
    }

    fn compute_sha256(&self, path: &Path) -> Result<String> {
        self.compute_sha256_with_progress(path, None)
    }

    fn compute_sha256_with_progress(
        &self,
        path: &Path,
        mut progress: Option<&mut dyn FnMut(DownloadProgress)>,
    ) -> Result<String> {
        let mut file = File::open(path)
            .with_context(|| format!("Failed to open {} for hashing", path.display()))?;
        let file_size = file.metadata().map(|m| m.len()).unwrap_or(0);
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 1024 * 64];
        let mut processed = 0u64;

        if let Some(cb) = progress.as_deref_mut() {
            cb(DownloadProgress {
                phase: DownloadPhase::VerifyingExisting,
                downloaded: processed,
                total: Some(file_size),
            });
        }

        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
            processed += read as u64;
            if let Some(cb) = progress.as_deref_mut() {
                cb(DownloadProgress {
                    phase: DownloadPhase::VerifyingExisting,
                    downloaded: processed,
                    total: Some(file_size),
                });
            }
        }

        Ok(format!("{:x}", hasher.finalize()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_parse_hf_model() {
        let model = HuggingFaceModel::parse("owner/repo:file.gguf").unwrap();
        assert_eq!(model.repo, "owner/repo");
        assert_eq!(model.revision, "main");
        assert_eq!(model.file, "file.gguf");
        assert_eq!(model.filename(), "file.gguf");
    }

    #[test]
    fn test_parse_hf_model_with_path() {
        let model = HuggingFaceModel::parse("owner/repo/path/to/file.gguf").unwrap();
        assert_eq!(model.repo, "owner/repo");
        assert_eq!(model.revision, "main");
        assert_eq!(model.file, "path/to/file.gguf");
        assert_eq!(model.filename(), "file.gguf");
    }

    #[test]
    fn test_parse_with_revision() {
        let model =
            HuggingFaceModel::parse("owner/repo@refs/pr/1:snapshots/file.bin").unwrap();
        assert_eq!(model.repo, "owner/repo");
        assert_eq!(model.revision, "refs/pr/1");
        assert_eq!(model.file, "snapshots/file.bin");
    }

    #[test]
    fn test_parse_explicit_filename() {
        let reference = "mradermacher/Luau-Qwen3-4B-FIM-v0.1-i1-GGUF:Luau-Qwen3-4B-FIM-v0.1.i1-Q4_K_M.gguf";
        let model = HuggingFaceModel::parse(reference).unwrap();
        assert_eq!(model.repo, "mradermacher/Luau-Qwen3-4B-FIM-v0.1-i1-GGUF");
        assert_eq!(model.file, "Luau-Qwen3-4B-FIM-v0.1.i1-Q4_K_M.gguf");
        assert_eq!(
            model.download_url(),
            "https://huggingface.co/mradermacher/Luau-Qwen3-4B-FIM-v0.1-i1-GGUF/resolve/main/Luau-Qwen3-4B-FIM-v0.1.i1-Q4_K_M.gguf?download=1"
        );
    }

    #[test]
    fn test_is_downloaded_checks_metadata() {
        let dir = tempdir().unwrap();
        let downloader = ModelDownloader::new(dir.path().to_path_buf());
        let model = HuggingFaceModel::parse("owner/repo:file.gguf").unwrap();

        let file_path = dir.path().join("file.gguf");
        fs::write(&file_path, b"hello world").unwrap();
        let sha = downloader.compute_sha256(&file_path).unwrap();
        let metadata_path = downloader.metadata_path("file.gguf");
        downloader
            .write_metadata(&metadata_path, &sha, Some("etag"))
            .unwrap();

        assert!(downloader.is_downloaded(&model));
    }

    #[test]
    fn test_download_url() {
        let model = HuggingFaceModel::parse("mradermacher/Luau-Qwen3-4B:Q4_K_M.gguf").unwrap();
        assert_eq!(
            model.download_url(),
            "https://huggingface.co/mradermacher/Luau-Qwen3-4B/resolve/main/Q4_K_M.gguf?download=1"
        );
    }
}
