# Quell — Project Guide

## What This Is

A Windows-native terminal proxy (and eventually standalone terminal) for AI CLI tools that eliminates scroll-jumping and flicker. It intercepts child process VT output via ConPTY, tracks screen state with a VT100 emulator, and sends only differential updates to the display.

## Research

Prior research lives in `research/` (gitignored, local only) organized by topic. Check `research/INDEX.md` for the full index.

## Architecture

```
User's Terminal ←→ quell (proxy) ←→ ConPTY ←→ AI CLI tool (e.g. Claude Code)
```

**Workspace layout:**
- `src/` — Shared engine library (used by CLI, GUI, tests, benches)
  - `proxy/` — Main proxy loop, I/O threads, event handling
  - `conpty/` — Windows ConPTY session management
  - `vt/` — VT100 emulation, sync block detection, differential rendering
  - `history/` — Scrollback history buffer with safe-replay filtering
  - `config/` — Configuration file loading and CLI args
- `cli/` — Phase 1 CLI binary (depends on `quell` library)
- `gui/` — Phase 2 Tauri GUI (placeholder, depends on `quell` library)

## Build & Run

```bash
cargo build                    # Debug build (production features only)
cargo build --release          # Release build
cargo build --features recording  # Dev build with VT recording support
cargo run -p quell-cli -- claude -- --dangerously-skip-permissions  # Run with Claude Code as child process
cargo test                     # Run all tests (default features)
cargo test --features recording  # Run all tests including recording
cargo test --test unit         # Unit tests only
cargo test --test integration  # Integration tests only
cargo bench                    # Run benchmarks
```

## Testing Requirements

Every feature needs unit tests + integration tests where applicable. Live-proving is tracked by automated hooks (see below).

**Test organization:**
- `tests/unit/` — Pure logic tests (VT parsing, diffing, sync detection, history filtering)
- `tests/integration/` — ConPTY spawning, pipe I/O, proxy end-to-end
- `benches/` — Performance benchmarks (VT diffing throughput, sync detection speed)

## Logging Standards

All modules use `tracing` with structured fields (not string interpolation). Levels: `error!` (unrecoverable), `warn!` (recovered), `info!` (lifecycle), `debug!` (frame-level), `trace!` (byte-level). Output to `logs/quell.log` and via `RUST_LOG` env var.

## Feature Flags

Dev-only functionality is gated behind Cargo feature flags to keep the production binary lean.

| Feature | Purpose | What it gates |
|---------|---------|---------------|
| `recording` | VT output capture for replay testing | `--record` CLI flag, `recorder.rs` module, `Proxy.recorder` field, hot-path recording hook |

**Rules:**
- Default features are empty — `cargo build` produces a clean production binary
- Dev/test builds use `cargo build --features recording` or `cargo test --features recording`
- **Never add runtime dependencies (Cargo.toml `[dependencies]`) for feature-gated code.** Use only stdlib. If a dep is truly needed, make it optional: `foo = { version = "X", optional = true }` and add it to the feature's dep list.
- **Never add unconditional code to the hot path** (the `recv(output_rx)` loop in `proxy/mod.rs`) for dev-only features. All hot-path additions must be behind `#[cfg(feature = "...")]`.
- New dev-only features must follow this same pattern: feature flag in `Cargo.toml`, `#[cfg]` on all production-path code.
- The release CI workflow (`cargo build --release`) must NOT enable dev features.

**Testing both configurations:**
```bash
cargo test                            # Default (no dev features) — must pass
cargo test --features recording       # With recording — must also pass
cargo clippy --lib                    # Default — no warnings
cargo clippy --lib --features recording  # With recording — no warnings
```

## Code Conventions

- **Rust 2024 edition** with stable toolchain
- **Error handling:** Use `anyhow::Result` for application code, `thiserror` for library errors
- **No unwrap() in non-test code** — use `?`, `.context()`, or explicit error handling
- **Naming:** snake_case for functions/variables, PascalCase for types, SCREAMING_SNAKE for constants
- **Module structure:** Each module has `mod.rs` with public API, internal files for implementation
- **Comments:** Only where the logic isn't self-evident. No boilerplate doc comments on obvious functions.

## UI/UX Conventions (Phase 2+)

- **Use the `/frontend-design` skill** when building or modifying GUI components to avoid generic "AI slop" aesthetics
- **Vanilla CSS with custom properties** for theming — no CSS frameworks. Single `theme.css` as source of truth.
- **Coordinate xterm.js theme object with CSS custom properties** — canvas-rendered terminal colors are independent of CSS, so the chrome and terminal palettes must be kept in sync manually
- **Dark theme defaults:** no pure black — use dark greys (`#1e1e1e` or tinted darks like `#0d1117`), desaturate bright colors, maintain WCAG 4.5:1 contrast minimum for body text
- **Font stack:** Cascadia Code, JetBrains Mono, Fira Code for terminal text; `system-ui` for window chrome
- **Aesthetic direction:** Industrial/utilitarian — sharp corners (2px max), panel splits over card layouts, subtle opacity transitions only

## Feature Workflow

1. Create/update tasks for the feature
2. Implement with tracing at decision points
3. **Integration audit** — for every `export function`/`pub fn`, verify external call sites exist. For every `listen()`/resource acquisition, verify cleanup exists. For every `invoke()`, verify the Rust command is registered.
4. Write unit tests (happy path + edge cases) and integration tests if I/O-touching
5. Commit — pre-commit hook enforces `cargo test` + `cargo clippy`, integration_audit hook checks wiring
6. Live-prove — hooks prompt for automated/automatable/manual categorization

## Planning Conventions

When planning multi-chunk features, each chunk specification must include:
- **Exports:** What functions/types this chunk creates for other modules to use
- **Wiring:** What call sites in OTHER modules must be added (not just "modify main.ts" but "call X() in Y()")
- **Cleanup:** For every activate/create/listen, specify the corresponding deactivate/destroy/unlisten
- **Trust boundaries:** Any function receiving external data (postMessage, IPC, user input) must validate argument shape

Plan by data flow, not just by component. "Build sdk-bridge.ts" is a component plan. "Theme changes propagate to plugins" is a data flow plan. Both are needed.

## Live-Proving Standard

Every live-proving checklist item must include all four parts:

1. **Setup** — what state to start from (e.g., "launch the app fresh with no tabs open")
2. **Action** — exact steps: what to click, type, or key-combo to press
3. **Expected result** — what you should see on screen, in devtools console, or in logs
4. **Pass/fail** — what constitutes a failure

Do not write vague steps like "verify error handling works". Write concrete steps like "click Open Folder → navigate to `C:\nonexistent` → click Select Folder → observe welcome screen reappears with no orphaned blank tab".

## Automated Hooks (`.claude/hooks/`)

These fire automatically — no manual action needed:

| Hook | Event | What It Does |
|------|-------|-------------|
| `planning_live_prove.py` | UserPromptSubmit | Injects live-proving checklist when planning features |
| `live_proving_reminder.py` | TaskCompleted | Reminds to categorize live-proving as automated/automatable/manual |
| `branch_phase_check.py` | PreToolUse (Edit/Write) | Warns if editing Phase 2 files on master or vice versa |
| `pre_commit_gate.py` | PreToolUse (Bash) | Blocks `git commit` if cargo test or clippy fails |
| `systematic_debugging.py` | Stop | Detects circular debugging (repeated corrections) and suggests structured approach |
| `gitignore_check.py` | PostToolUse (Write/Edit) | Warns if new files look sensitive, like build artifacts, or contain secrets |
| `session_git_status.py` | SessionStart | Injects repo state (unpushed commits, stale branches) as context |
| `push_reminder.py` | Stop | Warns about unpushed commits when Claude finishes responding |
| `commit_discipline.py` | PreToolUse (Bash) | Blocks pushing Phase 2 files to public origin, force push, and reset --hard |
| `integration_audit.py` | PostToolUse (Bash) | After git commit, checks for exported functions with no call sites and TS invoke() without matching Rust commands |

## Key Dependencies

| Crate | Purpose |
|-------|---------|
| `vt100` | VT100 terminal emulator + screen diffing (`contents_diff()`) |
| `memchr` | SIMD sync block marker detection |
| `vte` | Low-level VT escape sequence parser |
| `termwiz` | Escape sequence classification for history filtering |
| `windows` | Win32 API bindings (ConPTY, pipes, processes) |
| `tracing` | Structured logging throughout |
| `clap` | CLI argument parsing |
| `serde`/`toml` | Configuration file support |

## ConPTY Gotchas (Windows-Specific)

- ConPTY is NOT transparent — it re-encodes output through an internal buffer
- Unrecognized DCS sequences get swallowed
- Color resets (`ESC[39m`) can get mangled to `ESC[m` (full attribute reset)
- ConPTY generates spurious cursor/title sequences
- Input and output pipes MUST be on separate threads (deadlock risk)
- ~2 MiB/s throughput ceiling (adequate for Claude Code's ~189 KB/s peak)

## Lessons Learnt

### ConPTY HPCON handle passing (Critical)

The `windows` crate represents `HPCON` as `HPCON(pub isize)` — a newtype wrapping the raw handle value. When calling `UpdateProcThreadAttribute` with `PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE`, you must pass the **handle value itself** as the `lpValue` pointer, NOT a pointer to the handle variable:

```rust
// WRONG — passes address of the variable on the stack
let hpc_ptr = &hpc as *const HPCON as *const std::ffi::c_void;
UpdateProcThreadAttribute(..., Some(hpc_ptr), ...);

// CORRECT — passes the handle value directly (matching C API behavior)
let hpc_raw = hpc.0 as *const std::ffi::c_void;
UpdateProcThreadAttribute(..., Some(hpc_raw), ...);
```

The C API passes `HPCON` (which is `void*`) directly as `PVOID lpValue`. In the `windows` crate, HPCON wraps an `isize`, so you must extract the raw value with `.0`. Getting this wrong means the child process is created but never attached to the pseudoconsole — output silently goes to the parent's handles instead.

### STARTF_USESTDHANDLES with INVALID_HANDLE_VALUE

When the parent process has redirected stdout/stderr (common when running under IDEs, test frameworks, or piped environments), child processes spawned with ConPTY will inherit those redirected handles instead of using the pseudoconsole. Fix:

```rust
si.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
si.StartupInfo.hStdInput = INVALID_HANDLE_VALUE;
si.StartupInfo.hStdOutput = INVALID_HANDLE_VALUE;
si.StartupInfo.hStdError = INVALID_HANDLE_VALUE;
```

This forces Windows to NOT duplicate the parent's redirected handles to the child. Combined with the pseudoconsole attribute, the child correctly uses ConPTY for I/O.

### Rust 2024 edition: no static mut

Rust 2024 forbids `static mut` references. Use `std::sync::OnceLock` for global mutable state (e.g., Ctrl+C handler state). The `unsafe extern "system" fn` callback pattern works with OnceLock for signal handlers.

### windows crate API surface differences

The `windows` crate (0.59) wraps Win32 APIs differently from the `winapi` crate. Key differences to watch for:
- Many parameters wrapped in `Option<>` (e.g., `Some(0)` instead of bare `0` for reserved flags)
- `SetConsoleCtrlHandler` takes `Option<Option<fn>>` (PHANDLER_ROUTINE = Option<fn>)
- `HANDLE` is not `Send` — use `handle.0 as usize` to transfer across threads, then reconstruct
- `STARTUPINFOEXW::default()` zero-initializes, which may differ from `mem::zeroed()` for some fields

## Project Phases

- **Phase 1:** CLI proxy (shipped, public) — runs in any Windows terminal, eliminates scroll-jumping. Permanent product for power users.
- **Phase 2:** Standalone Tauri + xterm.js terminal (LOCAL ONLY — not pushed to GitHub until stable release). `commit_discipline.py` hook enforces this.
  - M2.1 Shell ✓, M2.2 Copy/Paste + Coalescer ✓, M2.3 Tabs + Restart ✓
  - M2.4 Architecture hardening (Channels IPC, keyboard shortcuts, zoom, addons, preferences)
- **Phase 3:** Public release — search, themes, command palette, then blocks, split panes, session persistence, community release.
  - 3.0 Search + Theme + Command Palette = **first public release gate**
  - 3.1 Block-based output + session persistence
  - 3.2 Split panes + polish
  - 3.3 Community release (auto-update, installer, push guard removal)
- **Phase 4:** Plugin marketplace — sandboxed plugin system, community marketplace, ecosystem growth.

## Plugin Marketplace (Phase 4)

Detailed research in `research/plugin-marketplace/INDEX.md`.

**Format:** `.qpk` (ZIP) packages containing `manifest.json` + HTML/CSS/JS. Plugins run in sandboxed iframes with a `postMessage`-based SDK bridge. Permissions are explicit and shown at install time.

**Mount points:** `panel-right`, `panel-bottom`, `tab`, `overlay`, `statusbar`, `decoration` (xterm.js layer), `background` (no UI).

**Marketplace approach:** Phased — starts as a GitHub-based registry (no server infrastructure), graduates to web portal if scale demands it. Plugins hosted as GitHub Releases on author repos, registry is a JSON index in a central repo.

**First-party plugin candidates:** AI Response Blocks (code block overlays), File Preview Panel, Git Dashboard, Diagram Renderer, Claude Voice (native integration of the claude-voice project), Terminal Multiplexer (split panes).

**SDK gaps identified:** `terminal:onInput` event (needed for autocomplete), deep-link URI scheme support (needed for OAuth flows like Spotify).
