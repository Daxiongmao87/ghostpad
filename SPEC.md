## 1. Document Control
_Conventions: Follow standard platform practices; research any unknown conventions via web search before implementation._

- **Product Name:** Ghostpad
- **Document Owner:** Patrick (requester) – update as needed
- **Version:** Draft 0.5
- **Last Updated:** 2025-12-11 (UTC)

## 2. Overview
_Conventions: Follow standard platform practices; research any unknown conventions via web search before implementation._

Ghostpad is a native Linux text editor focused on distraction-free writing paired with LLM-assisted auto-complete comparable to GitHub Copilot or Google Antigravity. The application must feel at home on GNOME, KDE, XFCE, and other desktops running under Wayland or Xorg by honoring their theming, shortcut, and portal conventions. The core experience centers on fast plaintext editing with first-class features (new/open/save, find/replace, line numbers, optional autosave) augmented by inline LLM suggestions that appear contextually without blocking manual typing.

## 3. Goals & Non-Goals
_Conventions: Follow standard platform practices; research any unknown conventions via web search before implementation._

**Goals**
- Deliver a lightweight, low-latency editor with reliable file handling (new/save/save-as/open/recent files).
- Provide inline LLM completions that feel native, are easy to accept or dismiss, and keep user data under explicit control.
- Respect desktop theming, accessibility, and input conventions across GNOME/KDE/XFCE on Wayland and Xorg.
- Include essential productivity tooling (find/replace, multi-cursor selection, go-to-line, word wrap options, line numbers).
- Support configurable autosave with safeguards against data loss and user confusion.

**Non-Goals**
- Implement a fully fledged IDE (no debugging, no project/workspace model).
- Become a cross-platform editor beyond Linux (Mac/Windows support out of scope for this release).
- Ship proprietary LLM models; Ghostpad will integrate with external providers via documented APIs.

## 4. User Personas & Key Use Cases
_Conventions: Follow standard platform practices; research any unknown conventions via web search before implementation._

1. **Ada – Power User Developer:** Needs a fast, keyboard-driven editor with AI suggestions for bash scripts, markdown notes, or quick experiments. Uses find/replace heavily and expects multi-monitor and theming fidelity on GNOME/Wayland.
2. **Sam – Technical Writer:** Works on long-form documentation, benefits from autosave snapshots and consistent typography, often runs XFCE/Xorg on older hardware.
3. **Lina – Student & Note Taker:** Values inline AI hints for syntax or phrasing, toggles between dark/light themes, and needs reassurance that drafts are safe on disk and not uploaded unexpectedly.

Key scenarios:
- Create/open/edit/save plaintext files with undo/redo, line numbers, optional minimap.
- Run find/replace with regex toggle, direction control, and replace-all preview.
- Toggle autosave intervals or manual-only mode, monitor status indicator.
- Trigger LLM completion manually (shortcut) or accept/dismiss inline greyed ghost text as they type.

## 5. Functional Requirements
_Conventions: Follow standard platform practices; research any unknown conventions via web search before implementation._

1. **File Operations**
   - New blank document, Open (file picker + drag/drop), Save, Save As, Save All for multi-tab future.
   - Recent files list (last 10) stored via XDG config.
   - Detect external changes (inotify) and prompt before overwriting.
2. **Editing**
   - Standard text editing with undo/redo stack, clipboard integration, multiple carets, indentation control.
   - Optional soft wrap, configurable tab width, show whitespace toggles.
   - Line numbers gutter, highlight current line, go-to-line dialog.
3. **Find & Replace**
   - Inline search panel with incremental highlights, regex toggle, match case, whole word, direction, and Replace/Replace All buttons.
4. **Autosave**
   - Timer-based autosave (default 5 minutes) with user-selectable interval options (off, 15s, 30s, 60s, 5m, custom) and status indicator.
   - Autosave writes to hidden swap file first, then atomically moves to target to prevent corruption.
   - Crash recovery prompts to restore latest snapshot on next launch.
5. **LLM Autocomplete**
   - Inline ghost text suggestions with accept (Tab/Right Arrow), dismiss (Esc), cycle (Alt+] / Alt+[).
   - Manual trigger command with context menu showing provider metadata plus current provider (local GGUF vs remote endpoint).
   - Status indicator for LLM connectivity/errors and whether a request is running or canceled.
   - Requests run through a low-latency debouncer (target <50 ms delay) and cancel immediately when the user resumes typing to keep UI responsive.
6. **Settings Persistence**
   - Use XDG config directory for JSON or TOML preferences, separate secret store for API tokens.

## 6. UX & UI Requirements
_Conventions: Follow standard platform practices; research any unknown conventions via web search before implementation._

- Single-window layout: header bar with primary actions, central editor pane backed by GtkSourceView-style component, bottom status bar for cursor position, autosave, LLM state.
- Follow GNOME HIG when running on GNOME (use libadwaita if available) while exposing portals/integration that keep accent colors/icons aligned on KDE/XFCE (respect icon theme via `org.freedesktop.appearance.color-scheme` portal and KDE color roles).
- Provide light/dark mode, font selection, and spacing options that follow system defaults on first run with overrides stored per user.
- Keyboard shortcuts mirror common Linux editors (Ctrl+N/O/S/F/H, Ctrl+Shift+F for replace, Ctrl+G for go-to-line, etc.).
- Inline LLM suggestions appear as translucent text using theme-appropriate muted color and do not shift layout.
- Provide toast or infobar messages instead of modal dialogs for background events (autosave success/failure, LLM errors) to keep flow uninterrupted.

## 7. LLM Autocomplete Feature
_Conventions: Follow standard platform practices; research any unknown conventions via web search before implementation._

- **Architecture:** Background service runs in separate async worker (Rust `tokio` task) that gathers context (current file content up to configurable token window, cursor location, optional user prompt) and sends requests to the selected provider via a plugin interface. Workers expose cooperative cancel so new keystrokes abort stale requests instantly.
- **Provider Abstraction:** Define trait for connectors and implement (a) **Local GGUF** using llama.cpp runtime; accept either a Hugging Face download URL (app downloads + caches under `$XDG_CACHE_HOME/ghostpad/models`) or a manual path to an existing `.gguf`, and (b) **Remote HTTP** connectors for OpenAI-compatible endpoints and Google Gemini APIs with configurable base URL/model. Ship official defaults (OpenAI `https://api.openai.com/v1` and Gemini `https://generativelanguage.googleapis.com/v1beta`) while allowing per-provider URL overrides for self-hosted gateways.
- **Compute Backends:** Local inference must auto-detect all supported accelerators and prefer them in this order: NVIDIA GPU (CUDA/cuBLAS build), AMD GPU (ROCm/HIP build), Intel Arc GPU (oneAPI/Level Zero build), and finally CPU (AVX2/NEON) as the last fallback. Settings must present a GPU section with a dropdown listing every detected GPU plus a `CPU Only` option; if no supported GPU exists, the dropdown is disabled, `CPU Only` is preselected, and GPU mode is unavailable. Users can override the automatic pick by explicitly selecting a GPU or forcing CPU-only inference. Default GGUF suggestions ship as `mradermacher/Luau-Qwen3-4B-FIM-v0.1-i1-GGUF:Q4_K_M` for GPU-backed inference and `OleFranz/Qwen3-0.6B-Text-FIM-GGUF` for CPU-only mode; allow per-backend override paths. Surface the active backend + model in UI, warn when VRAM/DRAM is insufficient, and immediately fall back to CPU if a chosen GPU fails during load.
- **Suggestion Surfacing:** Use FIM (fill-in-the-middle) generation with whole-suggestion delivery (no streaming). Debounce can begin on cursor movement, but completions are only injected once actual text input matches the pre-generated prefix; if the user types characters that continue the predicted text (e.g., space matching “This is **not**”), show the remaining ghost text immediately without re-requesting. When the typed input diverges (e.g., adding `n` to form “isn”), restart the ≤50 ms debounce and regenerate. Timeout after 2 seconds by default to avoid blocking typing.
- **User Controls:** Panel to view prompt/response history, clear context, pause/resume completions, pick provider mode (Local GGUF vs OpenAI vs Gemini), configure API keys or model paths, and opt in/out of telemetry.
- **Privacy:** No buffer content leaves machine unless a remote provider is explicitly configured and enabled. Provide redaction filters for long files (limit to last N lines or user-defined selection).
- **Edge Cases:** If offline, show discrete indicator and allow manual retry; fallback to cached completions history is out of scope for v1. Local mode surfaces GPU/CPU capability warnings when the selected GGUF exceeds hardware limits.

## 8. Architecture & Tech Stack
_Conventions: Follow standard platform practices; research any unknown conventions via web search before implementation._

- **Language:** Rust for core application logic (safety, performance). Use GTK 4 + GtkSourceView 5 for UI/editing, with libadwaita widgets when running on GNOME but degrade gracefully elsewhere.
- **Process Layout:** Single primary process; spawn async tasks for LLM communications and file watchers. Use `gio::Application` for lifecycle and DBus integration. Local GGUF inference runs inside a dedicated worker thread pinned to CPU cores or GPU command queues to avoid blocking GTK main loop; llama.cpp builds for CUDA, ROCm, oneAPI, and CPU ship side-by-side with runtime detection.
- **Desktop Integration:** Use `xdg-desktop-portal` for open/save dialogs and color-scheme detection so KDE/XFCE/Wayland sessions inherit native look/feel.
- **Packaging:** Provide Flatpak manifest (primary), AppImage for distros lacking Flatpak, and Debian package for apt-based systems. Flatpak build includes optional GGUF runtime dependencies (e.g., llama.cpp variants for CUDA/ROCm/oneAPI/CPU) via extensions so users only download needed backends.
- **Plugin Surface:** Expose JSON-RPC over Unix socket (future) for third-party LLM connectors; initial release bundles both local GGUF and HTTP connectors (OpenAI-compatible + Gemini).

## 9. Data Management & Autosave
_Conventions: Follow standard platform practices; research any unknown conventions via web search before implementation._

- Store preferences in `$XDG_CONFIG_HOME/ghostpad/config.toml`, state (recent files, window size) in `state.json`, and autosave buffers in `$XDG_STATE_HOME/ghostpad/autosave/`.
- Use atomic writes with temporary files and fsync to guarantee data durability.
- Provide retention policy for autosave snapshots (keep last N per file, default 5, purge older).
- Crash recovery dialog lists autosave files with timestamps and diff preview before restore.
- Optionally enable journaling logs for debugging autosave timing.

## 10. Configuration & Preferences
_Conventions: Follow standard platform practices; research any unknown conventions via web search before implementation._

- Settings dialog tabs: Editor, LLM, Theming, Shortcuts, Autosave.
- Provide UI for customizing fonts, indentation, whitespace visibility, wrapping, minimap toggles.
- LLM tab collects provider selection, endpoint URL (defaults to the official OpenAI/Gemini URLs but editable for overrides), model choice, auth token (stored via libsecret or kwallet), GGUF selection method (Hugging Face link with download progress or manual file picker), GPU device dropdown (detected GPUs plus CPU Only), override toggle for CPU-only inference, default model mapping per backend pre-filled with the Luau-Qwen3 GPU default and Qwen3 0.6B CPU default (both editable), and performance tuning knobs (max tokens, temperature, debounce interval).
- Autosave tab controls frequency, background save indicator style, and whether autosave occurs only when idle.
- Shortcut editor allows re-binding to match desktop conventions (KDE vs GNOME) and import/export as JSON.

## 11. Performance & Reliability
_Conventions: Follow standard platform practices; research any unknown conventions via web search before implementation._

- Target startup time < 1.5s on modern hardware; maintain editing latency under 16ms per keystroke with LLM disabled, <25ms with completions streaming.
- Idle CPU usage < 3% without LLM traffic; memory footprint < 200MB for 20k-line file.
- Implement incremental rendering and diff-based syntax highlighting to avoid full-buffer reflows.
- Add watchdog on LLM worker tasks; restart on panic/failure without crashing UI.

## 12. Security & Privacy
_Conventions: Follow standard platform practices; research any unknown conventions via web search before implementation._

- Never send file data to LLM providers without explicit configuration; show confirmation when enabling provider.
- Encrypt stored API tokens using OS keyring (libsecret on GNOME, KWallet on KDE, fallback to gnome-keyring service).
- Sanitize prompt data (strip secrets via regex list configurable by user).
- Use TLS with certificate pinning (optional) or system CA store; verify server responses, handle retries/backoff.
- Provide offline mode toggle to prevent outbound traffic entirely.

## 13. Telemetry & Logging
_Conventions: Follow standard platform practices; research any unknown conventions via web search before implementation._

- Default telemetry off; when enabled, capture anonymous aggregates (startup time, feature usage counts) and send via HTTPS abiding by distro policies.
- Provide log levels (error, warn, info, debug) with logs stored in `$XDG_STATE_HOME/ghostpad/logs/`.
- Include option to copy diagnostics bundle for bug reports (config, logs, environment info) with user approval.

## 14. Accessibility & Theming
_Conventions: Follow standard platform practices; research any unknown conventions via web search before implementation._

- Respect GTK accessibility APIs (AT-SPI), expose semantic descriptions for toolbar buttons, status indicators, and AI suggestion controls.
- Provide high-contrast theme option, adjustable line spacing, caret thickness, and screen reader hints for inline suggestions.
- Detect color scheme via portal (`org.freedesktop.appearance`) and follow prefer-dark setting; allow overrides.
- Ensure Wayland primary selection clipboard compatibility and fallback to X11 selection when under Xorg.
- Provide keyboard-only flows for every action, including LLM prompts and find/replace.

## 15. Open Questions & Risks
_Conventions: Follow standard platform practices; research any unknown conventions via web search before implementation._

- Launch scope is locked to local llama.cpp GGUF plus OpenAI-compatible and Google Gemini HTTP endpoints; no additional providers are planned for v1, but API cost analysis for these two services is still required.
- Streaming completions require robust cancellation; investigate server-sent events vs WebSockets per provider.
- Autosave frequency vs battery consumption on laptops—needs telemetry once available.
- GTK/libadwaita focus could feel alien on KDE; consider optional Qt-based front end or theming bridge if user feedback demands.
- Compliance review for telemetry and AI usage policies remains outstanding.
