use super::window::AppState;
use gtk4::prelude::*;
use libadwaita as adw;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionTrigger {
    Manual,
    Automatic,
}

impl AppState {
    pub(super) fn are_completions_suppressed(&self) -> bool {
        self.completion_suppression_depth.get() > 0
    }

    pub(super) fn with_suppressed_completion<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let depth = self.completion_suppression_depth.get();
        self.completion_suppression_depth.set(depth + 1);
        let result = f();
        self.completion_suppression_depth.set(depth);
        result
    }

    pub(super) fn request_llm_completion_with_generation(
        self: &Rc<Self>,
        trigger: CompletionTrigger,
        generation: u64,
    ) {
        // Check if this request is stale
        if generation != self.completion_generation.get() {
            return;
        }

        // Mark completion as in-flight
        if trigger == CompletionTrigger::Manual {
            self.manual_completion_inflight.set(true);
        } else {
            // Don't check auto_completion_running here - generation check handles staleness
            // Just set the flag to track that we're spawning
            self.auto_completion_running.set(true);
        }

        // Get the completion context (text before cursor)
        let context = self.completion_context();

        // Skip if context is empty
        if trigger == CompletionTrigger::Automatic && context.is_empty() {
            self.auto_completion_running.set(false);
            return;
        }

        // Show "Generating..." status
        self.status_label.set_text("Generating completion...");

        log::info!(
            "Triggering {:?} completion (generation {}), context length: {} chars, context_escaped: {:?}",
            trigger,
            generation,
            context.len(),
            context
        );

        // Prepare for background work
        let llm_manager = self.llm_manager.clone();
        let completion_generation = self.completion_generation.clone();
        
        // Determine if this is a FIM (fill-in-the-middle) request
        let is_fim = context.contains("<｜fim▁begin｜>");

        // Use a channel to communicate between threads
        let (tx, rx) = std::sync::mpsc::channel::<anyhow::Result<String>>();

        // Spawn thread to request completion
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<String> {
                // Check if stale BEFORE trying to lock (avoid wasting mutex time)
                if generation != completion_generation.get() {
                    log::info!("Completion request {} is stale, aborting before inference", generation);
                    return Err(anyhow::anyhow!("Request cancelled (generation mismatch)"));
                }

                let manager = llm_manager.lock().map_err(|e| {
                    anyhow::anyhow!("Failed to lock LLM manager: {}", e)
                })?;

                // Double-check after acquiring lock (in case it changed while waiting)
                if generation != completion_generation.get() {
                    log::info!("Completion request {} became stale while waiting for lock, aborting", generation);
                    return Err(anyhow::anyhow!("Request cancelled (generation mismatch after lock)"));
                }

                // Get max tokens from settings, but use smaller limit for FIM (mid-text) completion
                let max_tokens = if is_fim {
                    // FIM completions should be short - just filling a small gap
                    // Use max 50 tokens or settings value, whichever is smaller
                    std::cmp::min(50, manager.config().max_completion_tokens)
                } else {
                    manager.config().max_completion_tokens
                };

                log::info!("Running inference for generation {} (FIM={}, max_tokens={})", generation, is_fim, max_tokens);
                // Call the complete method
                let completion = manager.complete(&context, max_tokens)?;
                Ok(completion)
            })();

            let _ = tx.send(result);
        });

        // Set up receiver on main thread
        let weak = Rc::downgrade(self);
        gtk4::glib::idle_add_local(move || {
            // Stop polling if the window has been destroyed
            if weak.upgrade().is_none() {
                return gtk4::glib::ControlFlow::Break;
            }

            // Try to receive result
            match rx.try_recv() {
                Ok(result) => {
                    if let Some(state) = weak.upgrade() {
                        // Clear completion flags regardless of staleness
                        if trigger == CompletionTrigger::Manual {
                            state.manual_completion_inflight.set(false);
                        } else {
                            state.auto_completion_running.set(false);
                        }

                        // Check if this request is still current
                        if generation != state.completion_generation.get() {
                            return gtk4::glib::ControlFlow::Break;
                        }

                        match result {
                            Ok(completion_text) => {
                                // For FIM completions, trim trailing whitespace since they fill inline gaps
                                let completion_text = if is_fim {
                                    completion_text.trim_end().to_string()
                                } else {
                                    completion_text
                                };
                                
                                if !completion_text.trim().is_empty() {
                                    log::info!("Completion generated: {} chars", completion_text.len());
                                    // Show the completion as ghost text
                                    state.with_suppressed_completion(|| {
                                        state.document.insert_ghost_text(&completion_text);
                                    });
                                    state.status_label.set_text("Suggestion ready (Tab to accept, Esc to dismiss)");
                                } else {
                                    log::info!("Completion was empty");
                                    // Don't annoy user with "No completion generated"
                                    state.status_label.set_text("");
                                }
                            }
                            Err(err) => {
                                let err_msg = err.to_string();
                                // Don't show cancellation errors as failures
                                if err_msg.contains("Request cancelled") {
                                    log::debug!("Completion cancelled: {}", err);
                                    state.status_label.set_text("");
                                } else {
                                    log::warn!("LLM completion failed: {}", err);
                                    // Show error in status for all completions
                                    state.status_label.set_text(&format!("Completion error: {}", err));

                                    if trigger == CompletionTrigger::Manual {
                                        // Also show toast for manual completions
                                        let toast = adw::Toast::new(&format!("Completion failed: {}", err));
                                        toast.set_timeout(5);
                                        state.toast_overlay.add_toast(toast);
                                    }
                                }
                            }
                        }
                    } else {
                        // State dropped, clear flag anyway if we can't upgrade
                        log::warn!("State dropped while completion was running");
                    }
                    gtk4::glib::ControlFlow::Break
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    // Not ready yet, keep polling
                    gtk4::glib::ControlFlow::Continue
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    // Channel closed unexpectedly, clear flag
                    if let Some(state) = weak.upgrade() {
                        if trigger == CompletionTrigger::Manual {
                            state.manual_completion_inflight.set(false);
                        } else {
                            state.auto_completion_running.set(false);
                        }
                    }
                    gtk4::glib::ControlFlow::Break
                }
            }
        });
    }

    pub(super) fn preload_llm_model(self: &Rc<Self>) {
        // Show spinner and start it
        self.llm_spinner.show();
        self.llm_spinner.start();
        self.llm_status_label.show();
        self.llm_status_label.set_text("Loading LLM...");

        let llm_manager = self.llm_manager.clone();
        let (tx, rx) = std::sync::mpsc::channel::<anyhow::Result<()>>();

        // Spawn a background thread to preload the model
        std::thread::spawn(move || {
            log::info!("Starting background LLM model preload...");
            let result = (|| -> anyhow::Result<()> {
                let manager = llm_manager.lock().map_err(|e| {
                    anyhow::anyhow!("Failed to lock LLM manager: {}", e)
                })?;

                // Trigger model loading by requesting a dummy completion
                // This will download and load the model if needed
                let _ = manager.complete("test", 1)?;
                Ok(())
            })();

            let _ = tx.send(result);
        });

        // Poll for result on main thread
        let spinner = self.llm_spinner.clone();
        let status_label = self.llm_status_label.clone();
        let weak_for_trigger = Rc::downgrade(self);
        
        log::info!("Starting idle poller for LLM preload...");
        gtk4::glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            if weak_for_trigger.upgrade().is_none() {
                log::warn!("AppState dropped, stopping LLM preload poller");
                return gtk4::glib::ControlFlow::Break;
            }

            match rx.try_recv() {
                Ok(result) => {
                    log::info!("Received LLM preload result");
                    // Stop and hide spinner
                    spinner.stop();
                    spinner.hide();

                    match result {
                        Ok(()) => {
                            log::info!("LLM model preloaded successfully");
                            status_label.set_text("LLM ready");
                            // Hide the label after a few seconds
                            let label = status_label.clone();
                            gtk4::glib::timeout_add_seconds_local_once(3, move || {
                                label.hide();
                            });

                            // If user has typed something while loading, trigger completion
                            if let Some(weak_state) = weak_for_trigger.upgrade() {
                                // Check if there's text in the buffer
                                if weak_state.buffer.char_count() > 0 {
                                    log::info!("User was typing during LLM load, triggering auto-completion");
                                    // Schedule an auto-completion
                                    let generation = weak_state.bump_completion_generation();
                                    weak_state.schedule_auto_completion(generation);
                                }
                            }
                        }
                        Err(err) => {
                            log::warn!("Failed to preload LLM model: {}", err);
                            status_label.set_text("LLM unavailable");
                            // Keep the error visible
                        }
                    }
                    gtk4::glib::ControlFlow::Break
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    // Not ready yet, keep polling
                    // log::trace!("LLM preload still running...");
                    gtk4::glib::ControlFlow::Continue
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    log::error!("LLM preload thread channel disconnected!");
                    // Thread died unexpectedly
                    spinner.stop();
                    spinner.hide();
                    status_label.set_text("LLM load failed");
                    gtk4::glib::ControlFlow::Break
                }
            }
        });
    }
}
