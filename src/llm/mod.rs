use serde::{Deserialize, Serialize};

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

const DEFAULT_GPU_MODEL: &str = "mradermacher/Luau-Qwen3-4B-FIM-v0.1-i1-GGUF:Q4_K_M";
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
}

impl LlmManager {
    pub fn new(config: LlmSettings) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &LlmSettings {
        &self.config
    }

    pub fn update_config(&mut self, config: LlmSettings) {
        self.config = config;
    }

    pub fn detect_gpus() -> Vec<GpuDevice> {
        use std::process::Command;

        let output = Command::new("llama-cli")
            .arg("--list-devices")
            .output();

        let Ok(output) = output else {
            return Self::fallback_gpu_detection();
        };

        if !output.status.success() {
            return Self::fallback_gpu_detection();
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Self::parse_gpu_list(&stdout)
    }

    fn fallback_gpu_detection() -> Vec<GpuDevice> {
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

    fn parse_gpu_list(output: &str) -> Vec<GpuDevice> {
        let mut devices = Vec::new();

        for line in output.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("Available") {
                continue;
            }

            if let Some((id_part, name_part)) = line.split_once(':') {
                let id = id_part.trim().to_string();
                let name = name_part.trim().to_string();
                devices.push(GpuDevice { id, name });
            }
        }

        devices
    }
}
