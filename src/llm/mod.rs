use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub mod huggingface;
pub mod llamacpp;

pub use huggingface::{DownloadPhase, DownloadProgress, HuggingFaceModel, ModelDownloader};
pub use llamacpp::{InferenceConfig, LlamaCpp, LoadedModel};

#[derive(Debug, Clone, PartialEq)]
pub enum LlmReadiness {
    /// LLM is ready to use
    Ready,
    /// Local provider needs model download
    NeedsDownload { model_ref: String },
    /// Remote provider needs endpoint configuration
    NeedsEndpoint,
    /// llama-cli not found for local provider
    MissingLlamaCli,
    /// No provider configured
    NotConfigured,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProviderKind {
    OpenAI,
    Gemini,
    Local,
}

impl Default for ProviderKind {
    fn default() -> Self {
        ProviderKind::Local
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmSettings {
    pub provider: ProviderKind,
    pub endpoint: String,
    #[serde(default)]
    pub override_model_path: bool,
    pub local_model_path: String,
    #[serde(default)]
    pub preferred_device: Option<String>,
    #[serde(default)]
    pub force_cpu_only: bool,
    #[serde(default = "default_gpu_model")]
    pub default_gpu_model: String,
    #[serde(default = "default_cpu_model")]
    pub default_cpu_model: String,
}

impl Default for LlmSettings {
    fn default() -> Self {
        Self {
            provider: ProviderKind::Local,
            endpoint: "https://api.openai.com/v1".into(),
            override_model_path: false,
            local_model_path: String::new(),
            preferred_device: None,
            force_cpu_only: false,
            default_gpu_model: default_gpu_model(),
            default_cpu_model: default_cpu_model(),
        }
    }
}

const DEFAULT_GPU_MODEL: &str =
    "mradermacher/Luau-Qwen3-4B-FIM-v0.1-i1-GGUF:Luau-Qwen3-4B-FIM-v0.1.i1-Q4_K_M.gguf";
const DEFAULT_CPU_MODEL: &str = "OleFranz/Qwen3-0.6B-Text-FIM-GGUF";

fn default_gpu_model() -> String {
    DEFAULT_GPU_MODEL.to_string()
}

fn default_cpu_model() -> String {
    DEFAULT_CPU_MODEL.to_string()
}

#[derive(Debug, Clone)]
pub struct GpuDevice {
    pub id: String,
    pub name: String,
}

pub struct LlmManager {
    config: LlmSettings,
    models_dir: PathBuf,
    downloader: ModelDownloader,
    llamacpp: Option<Arc<LlamaCpp>>,
    loaded_model: Arc<Mutex<Option<LoadedModel>>>,
}

impl LlmManager {
    pub fn new(config: LlmSettings, models_dir: PathBuf) -> Self {
        let downloader = ModelDownloader::new(models_dir.clone());
        let llamacpp = LlamaCpp::new().ok().map(Arc::new);

        if llamacpp.is_none() {
            log::warn!("llama.cpp library failed to initialize - local inference will be unavailable");
        }

        Self {
            config,
            models_dir,
            downloader,
            llamacpp,
            loaded_model: Arc::new(Mutex::new(None)),
        }
    }

    pub fn config(&self) -> &LlmSettings {
        &self.config
    }

    pub fn update_config(&mut self, config: LlmSettings) {
        self.config = config;
    }

    /// Download a model from Hugging Face
    pub fn download_model(&self, model_ref: &str) -> anyhow::Result<PathBuf> {
        let model = HuggingFaceModel::parse(model_ref)?;
        self.downloader.download(&model)
    }

    pub fn downloader_handle(&self) -> ModelDownloader {
        self.downloader.clone()
    }

    /// Check if a model is downloaded
    pub fn is_model_downloaded(&self, model_ref: &str) -> bool {
        if let Ok(model) = HuggingFaceModel::parse(model_ref) {
            self.downloader.is_downloaded(&model)
        } else {
            false
        }
    }

    /// Get path to a downloaded model
    pub fn get_model_path(&self, model_ref: &str) -> Option<PathBuf> {
        if let Ok(model) = HuggingFaceModel::parse(model_ref) {
            self.downloader.get_path(&model)
        } else {
            None
        }
    }

    /// Load the configured model
    fn ensure_model_loaded(&self) -> anyhow::Result<()> {
        let llamacpp = self
            .llamacpp
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("llama.cpp not available"))?;

        // Check if already loaded
        if self.loaded_model.lock().unwrap().is_some() {
            return Ok(());
        }

        // Determine which model to use
        let model_path = if self.config.override_model_path
            && !self.config.local_model_path.is_empty()
        {
            // Use override path
            PathBuf::from(&self.config.local_model_path)
        } else {
            // Use default model based on GPU/CPU selection
            let model_ref = if self.config.force_cpu_only {
                &self.config.default_cpu_model
            } else {
                &self.config.default_gpu_model
            };

            // Ensure model is downloaded
            if !self.is_model_downloaded(model_ref) {
                log::info!("Model not downloaded, downloading: {}", model_ref);
                self.download_model(model_ref)?
            } else {
                self.get_model_path(model_ref)
                    .ok_or_else(|| anyhow::anyhow!("Model path not found"))?
            }
        };

        // Determine GPU layers
        let n_gpu_layers = if self.config.force_cpu_only {
            Some(0)
        } else {
            // Use all GPU layers by default when GPU is selected
            Some(999) // llama.cpp will use as many as possible
        };

        // Load the model
        log::info!("Loading model: {}", model_path.display());
        let loaded = llamacpp.load_model(&model_path, n_gpu_layers)?;

        *self.loaded_model.lock().unwrap() = Some(loaded);

        Ok(())
    }

    /// Run inference with the configured model
    pub fn complete(&self, prompt: &str, max_tokens: usize) -> anyhow::Result<String> {
        // Ensure model is loaded
        self.ensure_model_loaded()?;

        // Get the loaded model
        let model_lock = self.loaded_model.lock().unwrap();
        let model = model_lock
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Model not loaded"))?;

        // Run inference
        model.complete(prompt, max_tokens, 0.7)
    }

    /// Unload the current model
    pub fn unload_model(&self) {
        *self.loaded_model.lock().unwrap() = None;
    }

    /// Check if local inference is available
    pub fn is_local_available(&self) -> bool {
        self.llamacpp.is_some()
    }

    /// Check if LLM is ready for use
    pub fn check_readiness(&self) -> LlmReadiness {
        match self.config.provider {
            ProviderKind::Local => {
                // Check if llama-cli is available
                if !self.is_local_available() {
                    return LlmReadiness::MissingLlamaCli;
                }

                // Determine which model should be used
                let model_ref = if self.config.override_model_path
                    && !self.config.local_model_path.is_empty()
                {
                    // Check if override path exists
                    let path = PathBuf::from(&self.config.local_model_path);
                    if path.exists() {
                        return LlmReadiness::Ready;
                    } else {
                        return LlmReadiness::NeedsDownload {
                            model_ref: format!("Custom path: {}", self.config.local_model_path),
                        };
                    }
                } else {
                    // Use default model based on GPU/CPU selection
                    if self.config.force_cpu_only {
                        &self.config.default_cpu_model
                    } else {
                        &self.config.default_gpu_model
                    }
                };

                // Check if model is downloaded
                if self.is_model_downloaded(model_ref) {
                    LlmReadiness::Ready
                } else {
                    LlmReadiness::NeedsDownload {
                        model_ref: model_ref.to_string(),
                    }
                }
            }
            ProviderKind::OpenAI | ProviderKind::Gemini => {
                // Check if endpoint is configured
                if self.config.endpoint.is_empty() {
                    LlmReadiness::NeedsEndpoint
                } else {
                    // Assume ready - we can't validate without making a request
                    LlmReadiness::Ready
                }
            }
        }
    }

    /// Detect GPUs via system enumeration
    /// Note: llama.cpp will automatically detect and use GPUs at runtime
    /// This is just for UI display purposes
    pub fn detect_gpus() -> Vec<GpuDevice> {
        use std::fs;
        use std::path::Path;
        let mut devices = Vec::new();

        // Check for AMD GPUs via /sys/class/drm
        if let Ok(entries) = fs::read_dir("/sys/class/drm") {
            let mut card_count = 0;
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with("card") && !name_str.contains('-') {
                    // Try to read the device name
                    let vendor_path = entry.path().join("device/vendor");
                    let device_path = entry.path().join("device/device");

                    let vendor = fs::read_to_string(vendor_path).ok();
                    let device_name = if let Some(v) = vendor {
                        if v.trim() == "0x1002" {
                            "AMD GPU".to_string()
                        } else if v.trim() == "0x10de" {
                            "NVIDIA GPU".to_string()
                        } else if v.trim() == "0x8086" {
                            "Intel GPU".to_string()
                        } else {
                            format!("GPU {}", card_count)
                        }
                    } else {
                        format!("GPU {}", card_count)
                    };

                    devices.push(GpuDevice {
                        id: card_count.to_string(),
                        name: device_name,
                    });
                    card_count += 1;
                }
            }
        }

        // Fallback to simple check if nothing found
        if devices.is_empty() && Path::new("/dev/dri/card0").exists() {
            devices.push(GpuDevice {
                id: "0".to_string(),
                name: "GPU (detected via /dev/dri)".to_string(),
            });
        }

        devices
    }
}
