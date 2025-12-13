# Ghostpad (Wispnote)

**Ghostpad** (also known as **Wispnote**) is a lightweight, native Linux text editor with AI-powered autocomplete, designed to provide a distraction-free writing environment with local LLM integration.

## Project Overview

- **Purpose:** Native Linux editor with "Github Copilot-style" local AI suggestions.
- **Key Features:**
  - Local LLM inference (GGUF via llama.cpp).
  - Native GTK4/Libadwaita UI.
  - Autosave and crash recovery.
  - Distraction-free mode.
- **Status:** Alpha.

## Technical Stack

- **Language:** Rust (Edition 2024).
- **UI Framework:** GTK4 + Libadwaita (`gtk4`, `libadwaita` crates).
- **Editor Widget:** GtkSourceView 5 (`sourceview5` crate).
- **AI Backend:** `llama.cpp` (via `llama-cpp-2`).
- **Async Runtime:** Tokio (implied for background tasks).
- **Configuration:** TOML (settings), JSON (state).

## Architecture & Directory Structure

The project follows a modular Rust structure:

- **Entry Point:** `src/main.rs` - Initializes the `adw::Application` and logger.
- **Application Logic (`src/app/`):**
  - `mod.rs`: Orchestrates the UI build.
  - `window.rs`: Main window management.
  - `search.rs`: Find and replace functionality.
  - `preferences.rs`: Settings UI.
  - `completion.rs`: Handling of AI completion display/interaction.
- **Core Modules:**
  - `src/document.rs`: Document data model and manipulation.
  - `src/llm/`: Abstraction for AI backends (HuggingFace, LlamaCpp).
  - `src/settings.rs`: User preference persistence.
  - `src/state_store.rs`: App state (e.g., window size, recent files) persistence.
  - `src/paths.rs`: XDG path resolution.

## Development

### Prerequisites
- Rust 1.70+
- GTK4 & Libadwaita development libraries (e.g., `libgtk-4-dev`, `libadwaita-1-dev`).
- Vulkan SDK (optional, for GPU acceleration).

### Build & Run
```bash
# Build
cargo build

# Run
cargo run

# Build for Release
cargo build --release
```

### Testing
```bash
cargo test
```
*Note: Includes a smoke test for llama.cpp initialization (`tests/llama_backend_smoke.rs`).*

## Conventions

- **UI:** strictly adhere to GNOME HIG where possible using Libadwaita widgets.
- **Async:** Long-running tasks (especially LLM inference) MUST run in background threads/tasks to avoid freezing the GTK main loop.
- **Logging:** Use `log` macros (`info!`, `warn!`, `error!`). `env_logger` is initialized in `main`.
- **Formatting:** Standard `cargo fmt`.

## Critical Files

- `Cargo.toml`: Dependencies and metadata.
- `SPEC.md`: Detailed functional and technical requirements.
- `src/main.rs`: Application bootstrap.
- `.github/workflows/appimage.yml`: CI workflow for AppImage generation.
