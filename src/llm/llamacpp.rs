use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use anyhow::{anyhow, Context, Result};

/// Wrapper for llama.cpp CLI interactions
pub struct LlamaCpp {
    binary_path: PathBuf,
}

impl LlamaCpp {
    /// Find llama-cli in PATH or use bundled version
    pub fn new() -> Result<Self> {
        // Try to find llama-cli in PATH
        if let Ok(output) = Command::new("which").arg("llama-cli").output() {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout);
                let path = path.trim();
                if !path.is_empty() {
                    log::info!("Found llama-cli at: {}", path);
                    return Ok(Self {
                        binary_path: PathBuf::from(path),
                    });
                }
            }
        }

        // Try common install locations
        let candidates = vec![
            "/usr/bin/llama-cli",
            "/usr/local/bin/llama-cli",
            "/opt/llama.cpp/llama-cli",
        ];

        for candidate in candidates {
            if Path::new(candidate).exists() {
                log::info!("Found llama-cli at: {}", candidate);
                return Ok(Self {
                    binary_path: PathBuf::from(candidate),
                });
            }
        }

        Err(anyhow!(
            "llama-cli not found. Please install llama.cpp and ensure llama-cli is in PATH"
        ))
    }

    /// Run inference with a loaded model
    pub fn complete(
        &self,
        model_path: &Path,
        prompt: &str,
        max_tokens: usize,
        device: Option<&str>,
    ) -> Result<String> {
        let mut cmd = Command::new(&self.binary_path);

        cmd.arg("--model")
            .arg(model_path)
            .arg("--prompt")
            .arg(prompt)
            .arg("--n-predict")
            .arg(max_tokens.to_string())
            .arg("--temp")
            .arg("0.7")
            .arg("--ctx-size")
            .arg("2048")
            .arg("--log-disable")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Set device if specified
        if let Some(device_id) = device {
            cmd.arg("--device").arg(device_id);
        }

        log::debug!("Running llama-cli: {:?}", cmd);

        let output = cmd
            .output()
            .context("Failed to execute llama-cli")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("llama-cli failed: {}", stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        // llama-cli outputs the prompt + completion, so we need to strip the prompt
        let completion = stdout.trim();

        // Simple heuristic: find where the prompt ends
        if let Some(idx) = completion.find(prompt) {
            let result = &completion[idx + prompt.len()..];
            Ok(result.trim().to_string())
        } else {
            Ok(completion.to_string())
        }
    }

    /// Check if llama-cli can load a model (validation)
    pub fn validate_model(&self, model_path: &Path) -> Result<()> {
        if !model_path.exists() {
            return Err(anyhow!("Model file does not exist: {}", model_path.display()));
        }

        let output = Command::new(&self.binary_path)
            .arg("--model")
            .arg(model_path)
            .arg("--prompt")
            .arg("test")
            .arg("--n-predict")
            .arg("1")
            .arg("--log-disable")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .context("Failed to validate model")?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(anyhow!("Model validation failed: {}", stderr))
        }
    }

    /// Get the path to the llama-cli binary
    pub fn binary_path(&self) -> &Path {
        &self.binary_path
    }
}

/// Configuration for a single inference run
#[derive(Debug, Clone)]
pub struct InferenceConfig {
    pub model_path: PathBuf,
    pub prompt: String,
    pub max_tokens: usize,
    pub device: Option<String>,
}

impl InferenceConfig {
    pub fn new(model_path: PathBuf, prompt: String) -> Self {
        Self {
            model_path,
            prompt,
            max_tokens: 512,
            device: None,
        }
    }

    pub fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    pub fn with_device(mut self, device: String) -> Self {
        self.device = Some(device);
        self
    }
}
