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

- **Architecture:** Background service runs in a single async worker (Rust `tokio` task) that gathers context (current file content up to configurable token window, cursor location, optional user prompt) and sends requests to whichever provider is turned on. The worker immediately cancels any in-flight request when the user types again so stale suggestions never surface. Ship this module only after the baseline editor features in sections 5–6 are complete; until then the UI toggle stays hidden.
- **Provider Abstraction:** Define a minimal trait for connectors and implement (a) **Local GGUF** using llama.cpp runtime; if the user supplies a Hugging Face download URL and no local override is set, Ghostpad downloads that `.gguf` into `$XDG_DATA_HOME/ghostpad/models` and reuses it on future launches, and (b) **Remote HTTP** connectors for OpenAI-compatible endpoints and Google Gemini APIs with configurable base URL/model. Ship official defaults (OpenAI `https://api.openai.com/v1` and Gemini `https://generativelanguage.googleapis.com/v1beta`) while allowing per-provider URL overrides for self-hosted gateways.
- **Compute Backends:** Local inference must auto-detect all supported accelerators and prefer them in this order: NVIDIA GPU (CUDA/cuBLAS build), AMD GPU (ROCm/HIP build), Intel Arc GPU (oneAPI/Level Zero build), and finally CPU (AVX2/NEON) as the last fallback. Settings must present a GPU section with a dropdown listing every detected GPU plus a `CPU Only` option; if no supported GPU exists, the dropdown is disabled, `CPU Only` is preselected, and GPU mode is unavailable. Users can override the automatic pick by explicitly selecting a GPU or forcing CPU-only inference. Default GGUF suggestions ship as `mradermacher/Luau-Qwen3-4B-FIM-v0.1-i1-GGUF:Q4_K_M` for GPU-backed inference and `OleFranz/Qwen3-0.6B-Text-FIM-GGUF` for CPU-only mode; allow per-backend override paths. Surface the active backend + model in UI, warn when VRAM/DRAM is insufficient, and immediately fall back to CPU if a chosen GPU fails during load.
- **Suggestion Surfacing:** Use FIM (fill-in-the-middle) generation with whole-suggestion delivery (no streaming). Debounce can begin on cursor movement, but completions are only injected once actual text input matches the pre-generated prefix; if the user types characters that continue the predicted text (e.g., space matching “This is **not**”), show the remaining ghost text immediately without re-requesting. When the typed input diverges (e.g., adding `n` to form “isn”), restart the ≤50 ms debounce and regenerate. Timeout after 2 seconds by default to avoid blocking typing.
- **User Controls:** Panel to view prompt/response history, clear context, pause/resume completions, pick provider mode (Local GGUF vs OpenAI vs Gemini), configure API keys or model paths, and export local diagnostics bundles when support asks for them.
- **Privacy:** No buffer content leaves machine unless a remote provider is explicitly configured and enabled. Provide redaction filters for long files (limit to last N lines or user-defined selection).
- **Edge Cases:** If offline, show discrete indicator and allow manual retry; fallback to cached completions history is out of scope for v1. Local mode surfaces GPU/CPU capability warnings when the selected GGUF exceeds hardware limits.

## 8. Architecture & Tech Stack
_Conventions: Follow standard platform practices; research any unknown conventions via web search before implementation._

- **Language:** Rust for core application logic (safety, performance). Use GTK 4 + GtkSourceView 5 for UI/editing, with libadwaita widgets when running on GNOME but degrade gracefully elsewhere.
- **Process Layout:** Single primary process; spawn async tasks for LLM communications and file watchers. Use `gio::Application` for lifecycle and DBus integration. Local GGUF inference runs inside a dedicated worker thread pinned to CPU cores or GPU command queues to avoid blocking GTK main loop; llama.cpp builds for CUDA, ROCm, oneAPI, and CPU ship side-by-side with runtime detection.
- **Desktop Integration:** Use `xdg-desktop-portal` for open/save dialogs and color-scheme detection so KDE/XFCE/Wayland sessions inherit native look/feel.
- **Packaging:** Provide Flatpak manifest (primary), AppImage for distros lacking Flatpak, and Debian package for apt-based systems. Flatpak build includes optional GGUF runtime dependencies (e.g., llama.cpp variants for CUDA/ROCm/oneAPI/CPU) via extensions so users only download needed backends.
- **GPU Detection & Sandboxed Backends:** Flatpak builds rely on the standard `org.freedesktop.Platform.GL*` extensions; we detect whichever GPU llama.cpp can see inside the sandbox and fall back to CPU if no devices are exposed. If GPU access fails (driver missing or `/dev/dri` blocked), show a single warning telling users to check their Flatpak driver extensions or permissions rather than layering extra helpers. citeturn4search0turn0search1turn1search0
- **Host/Roaming Builds:** For AppImage and bare-metal installs, rely on llama.cpp’s device enumeration (including respect for `CUDA_VISIBLE_DEVICES` / `HIP_VISIBLE_DEVICES`) and display any permission failures inline; no extra CLI tooling.
- **llama.cpp Enumeration:** Bundle llama.cpp binaries that expose `llama-cli --list-devices`; run that command at startup to populate the GPU dropdown, honoring standard env overrides and falling back to CPU if nothing is available. citeturn3search1turn2search1
- **Source Layout:** Keep the Rust codebase modular to avoid another monolithic `app.rs`. Required structure:
  - `src/main.rs` – bootstrap only (create `Application`, call into `app::run()`).
  - `src/app/` – orchestrates windows and signals via focused files (`window.rs`, `menu.rs`, `recent.rs`, `find_replace.rs`, `autosave.rs`, `recovery.rs`, etc.). Each feature goes into its own module rather than bloating a single file.
  - `src/document/` – buffer/file helpers (`buffer.rs`, `file_ops.rs`, `autosave.rs`, `recovery.rs`) plus unit tests for persistence logic.
  - `src/widgets/` – reusable GTK/adw components (toolbar, status bar, preferences pages). Autosave preferences live in `src/widgets/preferences/autosave.rs`, LLM settings in `src/widgets/preferences/llm.rs`, etc.
  - Shared utilities (`paths.rs`, `settings.rs`, `state_store.rs`) remain standalone modules.
  - New functionality must conform to this layout; pull requests that expand the orchestration file instead of adding/using modules are rejected until they follow the structure.

## 9. Data Management & Autosave
_Conventions: Follow standard platform practices; research any unknown conventions via web search before implementation._

- Store preferences in `$XDG_CONFIG_HOME/ghostpad/config.toml`, state (recent files, window size) in `state.json`, and autosave buffers in `$XDG_STATE_HOME/ghostpad/autosave/`.
- Use atomic writes with temporary files and fsync to guarantee data durability.
- Autosave is a simple timer (default 60 s with optional 15 s and 5 m presets). Each save writes to a hidden swap file per document plus one rolling backup snapshot; no retention policies or background journals.
- After an unexpected exit, prompt once with the timestamp of the available backup so the user can restore or discard it.

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

## 13. Logging & Diagnostics
_Conventions: Follow standard platform practices; research any unknown conventions via web search before implementation._

- Ghostpad never transmits analytics/telemetry. All runtime logs stay local under `$XDG_STATE_HOME/ghostpad/logs/` with log levels (error, warn, info, debug) to aid troubleshooting.
- Provide an explicit “Export diagnostics” button that zips config snapshots + recent logs so users can choose to share them manually when filing bugs.

## 14. Accessibility & Theming
_Conventions: Follow standard platform practices; research any unknown conventions via web search before implementation._

- Respect GTK accessibility APIs (AT-SPI), expose semantic descriptions for toolbar buttons, status indicators, and AI suggestion controls.
- Provide high-contrast theme option, adjustable line spacing, caret thickness, and screen reader hints for inline suggestions.
- Detect color scheme via portal (`org.freedesktop.appearance`) and follow prefer-dark setting; allow overrides.
- Ensure Wayland primary selection clipboard compatibility and fallback to X11 selection when under Xorg.
- Provide keyboard-only flows for every action, including LLM prompts and find/replace.

## 15. Open Questions & Risks
_Conventions: Follow standard platform practices; research any unknown conventions via web search before implementation. Items below explicitly require additional research._

- **API provider cost baselines (answered 2025-12-11).** OpenAI’s latest pricing puts `gpt-4o-mini` at $0.15/1M input tokens ($0.075 cached) and $0.60/1M output tokens, while `gpt-4.1-mini` comes in at $0.40/$1.60; higher-end GPT-5.x models start at $1.25/$10.00 per 1M tokens. Gemini `2.5 Flash-Lite` remains the most cost-effective Google option at $0.10/1M multimodal input tokens and $0.40/1M output tokens (batch mode halves the input price). Ghostpad should default to a “cost aware” preset (gpt-4o-mini + Gemini Flash-Lite) and warn users before enabling providers that exceed $1/1M tokens. ([OpenAI pricing, crawled 2025-12-05](https://platform.openai.com/pricing); [Gemini API pricing, crawled 2025-12-09](https://ai.google.dev/gemini-api/docs/pricing))
  - _Action:_ Surface these reference numbers directly in the provider picker and logs so users can track projected vs. actual spend.
- **Transport for completions (answered 2025-12-11).** For FIM-style completions we still insert suggestions atomically, but we now treat transport streaming as a responsiveness primitive: OpenAI’s Responses API emits typed Server-Sent Events when `stream=true`, which gives us mid-flight cancellation hooks and status/error events without token-by-token UI. Gemini Live, in contrast, requires WebSockets (client-to-server or server-to-server) for low-latency transfers. SSE stays the default for “ghost text,” while WebSockets are reserved for interactive voice/video sessions or Gemini Live fallbacks. ([OpenAI streaming guide](https://platform.openai.com/docs/guides/streaming-responses), [WebSocket vs. SSE comparison](https://websocket.org/comparisons/sse/), [Gemini Live API](https://ai.google.dev/gemini-api/docs/live))
  - _Action:_ Build a transport abstraction that chooses SSE for REST providers, WebSockets for Live sessions, and documents the cancellation semantics in developer diagnostics.
- **Autosave energy impact (answered 2025-12-11).** University of Tartu’s “Hidden Cost of Autosave” thesis profiled 900 runs across Mu, Leo, and novelWriter and found that simply reducing polling frequency on quiet buffers cut Mu’s autosave energy draw by up to 83%. We’ll stick to the straightforward timer-based autosave already described (default 60 s with optional 15 s/5 m presets), always write to a hidden swap file, and keep one rolling backup snapshot per document for crash recovery—no extra heuristics or burst modes. ([The Hidden Cost of Autosave, 2025](https://dspace.ut.ee/items/c2b3c2ce-5e39-413a-a455-542b672db2d3))
  - _Action:_ Document the swap-file + single-backup flow in settings/help so users understand when backups are created and restored.
- **GPU detection & packaging (answered 2025-12-11).** Flatpak already exposes GPUs through the `org.freedesktop.Platform.GL*` extension system (`flatpak --gl-drivers` lists the Mesa “default” plus any vendor driver); NVIDIA users must match the extension’s version to their kernel driver, while AMD/Intel rely on Mesa plus access to `/dev/dri/renderD*`. AppImage builds lack these helpers, so Ghostpad must probe `/dev/dri` directly, respect PRIME env vars, and show remediation steps borrowed from Sunshine/Kdenlive bug threads when VAAPI permissions fail. llama.cpp’s `llama-cli --list-devices` (and env vars such as `HIP_VISIBLE_DEVICES` / `CUDA_VISIBLE_DEVICES`) give us a portable way to enumerate GPUs before populating the dropdown described in sections 7 and 10. ([Flatpak GL extensions](https://docs.flatpak.org/en/latest/extension.html); [Flathub driver mismatch thread](https://discourse.flathub.org/t/when-i-update-freedesktop-one-of-my-flatpak-app-loses-video/6438); [GTK apps default Mesa note](https://www.reddit.com/r/Fedora/comments/12xq1lx/flatpak_gl_drivers/); [Sunshine VAAPI permissions issue](https://github.com/LizardByte/Sunshine/issues/2409); [llama.cpp device detection guide](https://insightpulse.io/llamastream/llama-cpp-guide/#llama-cli-help); [llama.cpp env overrides](https://github.com/NousResearch/llama.cpp/blob/main/docs/install.md))
  - _Action:_ Wire these detection steps into startup diagnostics, surface warnings inside the GPU picker, and document Flatpak/AppImage-specific fixes in the help center.
- **KDE/libadwaita UX parity (ongoing).** KDE community guidance shows that Plasma users currently rely on targeted GTK4 CSS (e.g., adjusting window control radii) and tools like Gradience/rewaita, but official theming hooks remain limited because libadwaita intentionally resists wholesale overrides. Ghostpad must therefore detect the portal-provided accent/dark-mode hints and expose per-desktop toggles (e.g., “Use Plasma colors where available”) rather than promising full native theming on KDE. ([KDE Discuss: plasma CSS tweaks, Mar 2025](https://discuss.kde.org/t/approach-to-make-libadwaita-apps-look-native-on-plasma/31428); [KDE Discuss: theming libadwaita in KDE, Jun 2025](https://discuss.kde.org/t/theming-libadwaita-in-kde/34963); [Reddit /r/kde thread on GTK4 theming limits, Nov 2025](https://www.reddit.com/r/kde/comments/1p8u0vb/gtk_application_style_does_nothing/))
  - _Action:_ Implement a “Plasma-friendly” mode that proxies the color portal, applies safe CSS overrides (button radius, header-bar contrast), and clearly labels anything beyond that as experimental.
- **AI compliance & transparency obligations (ongoing).** The EU AI Act entered into force on 1 Aug 2024; prohibited practices are enforceable since Feb 2025, GPAI/penalty provisions apply from Aug 2025, and transparency/high-risk duties come due Aug 2026–Aug 2027. Providers face fines up to €35 M/7% revenue, and GPAI deployers must log activity, expose human-oversight affordances, and label AI-generated content. The Commission is also drafting transparency codes of practice (consultation closes Oct 2025), so Ghostpad must provide clear in-app AI disclosures, keep audit logs locally under `$XDG_STATE_HOME/ghostpad/logs/`, and rely solely on user-initiated diagnostics exports—no telemetry beacons. ([EU Digital Strategy AI Act timeline](https://digital-strategy.ec.europa.eu/en/policies/regulatory-framework-ai); [EU transparency code consultation, Sep 2025](https://digital-strategy.ec.europa.eu/en/news/commission-launches-consultation-develop-guidelines-and-code-practice-transparent-ai-systems); [DLA Piper summary of Aug 2025 obligations](https://www.dlapiper.com/en-lu/insights/publications/2025/08/latest-wave-of-obligations-under-the-eu-ai-act-take-effect); [Jones Day penalty overview, Feb 2025](https://www.jonesday.com/es/insights/2025/02/eu-ai-act-first-rules-take-effect-on-prohibited-ai-systems))
  - _Action:_ Add an “AI disclosures” checklist to release gating (human-in-the-loop controls, exportable logs, provider labeling) and revisit before Aug 2025 and Aug 2026 deadlines.
