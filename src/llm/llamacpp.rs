use anyhow::{Result, anyhow};
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel, Special};
use llama_cpp_2::sampling::LlamaSampler;
use std::path::Path;
use std::sync::Arc;

/// Wrapper for llama.cpp library with in-process inference
pub struct LlamaCpp {
    backend: Arc<LlamaBackend>,
}

impl LlamaCpp {
    /// Initialize llama.cpp backend
    pub fn new() -> Result<Self> {
        let backend = LlamaBackend::init()
            .map_err(|e| anyhow!("Failed to initialize llama.cpp backend: {:?}", e))?;

        Ok(Self {
            backend: Arc::new(backend),
        })
    }

    /// Load a model from disk
    pub fn load_model(&self, model_path: &Path, n_gpu_layers: Option<i32>, main_gpu: Option<i32>) -> Result<LoadedModel> {
        if !model_path.exists() {
            return Err(anyhow!(
                "Model file does not exist: {}",
                model_path.display()
            ));
        }

        let mut params = LlamaModelParams::default();

        if let Some(layers) = n_gpu_layers {
            let layers_u32 = u32::try_from(layers)
                .map_err(|_| anyhow!("GPU layers must be zero or positive"))?;
            log::info!("Setting n_gpu_layers = {}", layers_u32);
            params = params.with_n_gpu_layers(layers_u32);
        }

        if let Some(gpu) = main_gpu {
            log::info!("Setting main_gpu = {}", gpu);
            params = params.with_main_gpu(gpu);
        } else {
            log::warn!("main_gpu is None - no GPU device specified!");
        }

        let model = LlamaModel::load_from_file(&self.backend, model_path, &params)
            .map_err(|e| anyhow!("Failed to load model: {:?}", e))?;

        Ok(LoadedModel {
            backend: Arc::clone(&self.backend),
            model: Arc::new(model),
        })
    }
}

/// A loaded model ready for inference
pub struct LoadedModel {
    backend: Arc<LlamaBackend>,
    model: Arc<LlamaModel>,
}

impl LoadedModel {
    /// Run inference with the loaded model
    pub fn complete(&self, prompt: &str, max_tokens: usize, temperature: f32) -> Result<String> {
        // Create context
        let ctx_params = LlamaContextParams::default().with_n_ctx(std::num::NonZeroU32::new(2048));

        let mut ctx = self
            .model
            .new_context(&self.backend, ctx_params)
            .map_err(|e| anyhow!("Failed to create context: {:?}", e))?;

        // Tokenize the prompt
        let tokens = self
            .model
            .str_to_token(prompt, AddBos::Always)
            .map_err(|e| anyhow!("Failed to tokenize prompt: {:?}", e))?;

        if tokens.is_empty() {
            return Err(anyhow!("Tokenization resulted in empty token sequence"));
        }

        let n_ctx = ctx.n_ctx() as usize;
        let n_prompt = tokens.len();

        if n_prompt >= n_ctx {
            return Err(anyhow!(
                "Prompt too long: {} tokens exceeds context size {}",
                n_prompt,
                n_ctx
            ));
        }

        // Prepare batch for prompt processing
        let mut batch = LlamaBatch::new(n_ctx, 1);

        for (i, &token) in tokens.iter().enumerate() {
            let is_last = i + 1 == tokens.len();
            batch
                .add(token, i as i32, &[0], is_last)
                .map_err(|e| anyhow!("Failed to add token to batch: {:?}", e))?;
        }

        // Process the prompt
        ctx.decode(&mut batch)
            .map_err(|e| anyhow!("Failed to decode prompt: {:?}", e))?;

        // Generate tokens
        let mut result = String::new();
        let mut n_cur = n_prompt;
        let n_max = n_prompt + max_tokens;

        let mut sampler =
            LlamaSampler::chain_simple([LlamaSampler::temp(temperature), LlamaSampler::greedy()]);

        while n_cur < n_max {
            // Sample next token
            let logits_index = batch.n_tokens() - 1;
            let mut candidates_data = ctx.token_data_array_ith(logits_index);
            sampler.apply(&mut candidates_data);
            let new_token_id = sampler.sample(&ctx, logits_index);
            sampler.accept(new_token_id);

            // Check for EOS
            if self.model.is_eog_token(new_token_id) {
                break;
            }

            // Use Tokenize to handle special tokens (like FIM sentinels) if Plaintext fails
            let piece = match self.model.token_to_str(new_token_id, Special::Tokenize) {
                Ok(s) => s,
                Err(e) => {
                    log::warn!("Failed to decode token {}: {:?} (skipping)", new_token_id, e);
                    continue;
                }
            };

            // Filter out FIM sentinels if they leak into generation
            if piece.contains("<|fim_") || piece.contains("<|file_sep|>") {
                continue;
            }

            result.push_str(&piece);

            // Prepare next batch
            batch.clear();
            batch
                .add(new_token_id, n_cur as i32, &[0], true)
                .map_err(|e| anyhow!("Failed to add token to batch: {:?}", e))?;

            // Decode
            ctx.decode(&mut batch)
                .map_err(|e| anyhow!("Failed to decode: {:?}", e))?;

            n_cur += 1;
        }

        Ok(result)
    }
}
