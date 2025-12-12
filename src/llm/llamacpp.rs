use anyhow::{Result, anyhow};
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel, Special};
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;
use std::path::{Path, PathBuf};
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

        log::info!("Model loaded successfully");

        Ok(LoadedModel {
            backend: Arc::clone(&self.backend),
            model: Arc::new(model),
            source_path: model_path.to_path_buf(),
        })
    }
}

/// A loaded model ready for inference
pub struct LoadedModel {
    backend: Arc<LlamaBackend>,
    model: Arc<LlamaModel>,
    pub source_path: PathBuf,
}

impl LoadedModel {
    /// Run inference with the loaded model
    pub fn complete(&self, prompt: &str, max_tokens: usize, temperature: f32) -> Result<String> {
        // Start heartbeat thread to track execution
        let heartbeat_running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let heartbeat_flag = heartbeat_running.clone();
        let heartbeat_thread = std::thread::spawn(move || {
            eprintln!("[HEARTBEAT] Thread started, will print every 2 seconds...");
            let mut count = 0;
            while heartbeat_flag.load(std::sync::atomic::Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_secs(2));
                if heartbeat_flag.load(std::sync::atomic::Ordering::Relaxed) {
                    count += 2;
                    eprintln!("[HEARTBEAT] LLM still running after {}s...", count);
                }
            }
            eprintln!("[HEARTBEAT] Thread stopping...");
        });

        let result = self.complete_inner(prompt, max_tokens, temperature);
        
        // Stop heartbeat
        heartbeat_running.store(false, std::sync::atomic::Ordering::Relaxed);
        let _ = heartbeat_thread.join();
        
        result
    }

    fn complete_inner(&self, prompt: &str, max_tokens: usize, temperature: f32) -> Result<String> {
        // Create context
        let ctx_params = LlamaContextParams::default().with_n_ctx(std::num::NonZeroU32::new(2048));

        let mut ctx = self
            .model
            .new_context(&self.backend, ctx_params)
            .map_err(|e| anyhow!("Failed to create context: {:?}", e))?;

        // Log if this is a FIM prompt
        // DeepSeek uses <｜fim▁begin｜> format (U+2581 character, not space)
        let is_fim = prompt.contains("<｜fim▁begin｜>") || prompt.contains("<|fim_prefix|>");
        if is_fim {
            eprintln!("=== FIM COMPLETION REQUEST ===");
            eprintln!("FIM prompt detected");
            // Log first 200 chars of prompt for debugging
            let preview: String = prompt.chars().take(200).collect();
            eprintln!("Prompt preview: {:?}", preview);
        } else {
             eprintln!("=== STANDARD COMPLETION REQUEST (No FIM markers) ===");
        }
        
        // llama-cpp-2's str_to_token already has parse_special=true hardcoded,
        // so special tokens like <|fim_prefix|> will be parsed correctly
        let tokens = self.model
            .str_to_token(prompt, AddBos::Always)
            .map_err(|e| anyhow!("Failed to tokenize prompt: {:?}", e))?;

        eprintln!("Tokenized prompt into {} tokens", tokens.len());
        
        // Check BOS
        let bos = self.model.token_bos();
        eprintln!("Model BOS token: {:?}", bos);
        
        // Log first few tokens to see if special tokens are being parsed correctly
        if tokens.len() > 0 {
            let first_tokens: Vec<_> = tokens.iter().take(10).collect();
            eprintln!("First 10 tokens: {:?}", first_tokens);
            
            if tokens[0] != bos {
                eprintln!("WARNING: First token is NOT BOS! Expected {:?}, got {:?}", bos, tokens[0]);
            }

            // Try to decode first few tokens to see what they represent
            for (i, token) in tokens.iter().take(5).enumerate() {
                if let Ok(s) = self.model.token_to_str(*token, Special::Tokenize) {
                    eprintln!("Token {}: {:?} -> {:?}", i, token, s);
                }
            }
        }

        if tokens.is_empty() {
             eprintln!("ERROR: Tokenization resulted in empty token sequence");
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
        eprintln!("Starting prompt decode with {} tokens...", n_prompt);
        ctx.decode(&mut batch)
            .map_err(|e| anyhow!("Failed to decode prompt: {:?}", e))?;
        eprintln!("Prompt decode complete, starting generation...");

        // Generate tokens
        let mut result = String::new();
        let mut n_cur = n_prompt;
        let n_max = n_prompt + max_tokens;

        let mut sampler =
            LlamaSampler::chain_simple([LlamaSampler::temp(temperature), LlamaSampler::greedy()]);
        
        eprintln!("Generation loop: n_cur={}, n_max={}", n_cur, n_max);
        let gen_start = std::time::Instant::now();

        while n_cur < n_max {
            let token_start = std::time::Instant::now();
            
            // Sample next token directly
            let logits_index = batch.n_tokens() - 1;
            let new_token_id = sampler.sample(&ctx, logits_index);
            sampler.accept(new_token_id);

            // Logging for debugging - show time per token
            if let Ok(s) = self.model.token_to_str(new_token_id, Special::Tokenize) {
                eprintln!("Generated token: {:?} -> {:?} ({}ms)", new_token_id, s, token_start.elapsed().as_millis());
            }

            // Check for EOS
            if self.model.is_eog_token(new_token_id) {
                if is_fim {
                     eprintln!("EOS token detected, stopping generation");
                }
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
            // Supports both Qwen/StarCoder style (<|fim_*|>) and DeepSeek style (<｜fim▁*｜>)
            if piece.contains("<|fim_") || piece.contains("<|file_sep|>") || piece.contains("<｜fim") {
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

        eprintln!("[{:?}] Generation complete, {} tokens generated", std::time::SystemTime::now(), n_cur - n_prompt);
        Ok(result)
    }
}
