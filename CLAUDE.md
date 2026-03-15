# Quell — Project Guide

## What This Is

A Windows-native terminal proxy (and eventually standalone terminal) for AI CLI tools that eliminates scroll-jumping and flicker. It intercepts child process VT output via ConPTY, tracks screen state with a VT100 emulator, and sends only differential updates to the display.

## Research

Prior research lives in `research/` (gitignored, local only) organized by topic. Check `research/INDEX.md` for the full index.

## Architecture

```
User's Terminal ←→ quell (proxy) ←→ ConPTY ←→ AI CLI tool (e.g. Claude Code)
```

Core modules:
- `src/proxy/` — Main proxy loop, I/O threads, event handling
- `src/conpty/` — Windows ConPTY session management (CreatePseudoConsole, pipes, resize)
- `src/vt/` — VT100 emulation, sync block detection, differential rendering
- `src/history/` — Scrollback history buffer with safe-replay filtering
- `src/config/` — Configuration file loading and CLI args

## Build & Run

```bash
cargo build                    # Debug build
cargo build --release          # Release build
cargo run -- claude -- --dangerously-skip-permissions  # Run with Claude Code as child process
cargo test                     # Run all tests
cargo test --test unit         # Unit tests only
cargo test --test integration  # Integration tests only
cargo bench                    # Run benchmarks
```

## Testing Requirements

**Every feature must have:**
1. Unit tests covering core logic and edge cases
2. Integration tests verifying end-to-end behavior where applicable
3. Live-proving against actual Claude Code before marking as done — automated tests validate correctness but real-world behavior under streaming load is the true acceptance test

**Test organization:**
- `tests/unit/` — Pure logic tests (VT parsing, diffing, sync detection, history filtering)
- `tests/integration/` — ConPTY spawning, pipe I/O, proxy end-to-end
- `benches/` — Performance benchmarks (VT diffing throughput, sync detection speed)

**Running tests:**
```bash
cargo test                          # All tests
cargo test --test unit              # Unit tests
cargo test --test integration       # Integration tests (may need admin on some systems)
RUST_LOG=debug cargo test           # Tests with log output
```

## Logging Standards

**All modules must use `tracing` for structured logging.** This is non-negotiable — diagnosis is always faster with good logs.

**Log levels:**
- `error!` — Unrecoverable failures (ConPTY creation failed, pipe broken)
- `warn!` — Recoverable issues (malformed VT sequence skipped, ConPTY noise filtered)
- `info!` — Lifecycle events (proxy started, child spawned, resize, session ended)
- `debug!` — Frame-level data (render triggered, diff size, sync block detected)
- `trace!` — Byte-level data (raw VT sequences, individual cell changes)

**Log format:**
- Structured fields: `tracing::info!(bytes = data.len(), elapsed_ms = elapsed, "render complete")`
- File output: `logs/quell.log` (rotated, configurable)
- Console output: Controlled by `RUST_LOG` env var

**When adding a feature, you MUST add:**
1. `info!` log at the feature's entry/exit points
2. `debug!` logs for decision points and state changes
3. `warn!` logs for anything unexpected but handled
4. Structured fields (not string interpolation) for machine-parseable logs

## Code Conventions

- **Rust 2024 edition** with stable toolchain
- **Error handling:** Use `anyhow::Result` for application code, `thiserror` for library errors
- **No unwrap() in non-test code** — use `?`, `.context()`, or explicit error handling
- **Naming:** snake_case for functions/variables, PascalCase for types, SCREAMING_SNAKE for constants
- **Module structure:** Each module has `mod.rs` with public API, internal files for implementation
- **Comments:** Only where the logic isn't self-evident. No boilerplate doc comments on obvious functions.

## Feature Workflow

1. Create/update tasks for the feature
2. Implement the feature with logging at all decision points
3. Write unit tests covering happy path + edge cases
4. Write integration tests if the feature touches I/O or ConPTY
5. Run `cargo test` — all tests must pass
6. Run `cargo clippy` — no warnings
7. Live-prove against Claude Code — verify behavior under real streaming load
8. Only then mark the feature as done

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

- **Phase 1:** CLI proxy (current) — runs in any Windows terminal, eliminates scroll-jumping. Permanent product for power users.
- **Phase 2:** Standalone Tauri + xterm.js terminal with structured output, tabs, accessibility, themes
- **Phase 3:** Session persistence, search, split panes, community release, auto-update
