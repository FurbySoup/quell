# quell

[![CI](https://github.com/FurbySoup/quell/actions/workflows/ci.yml/badge.svg)](https://github.com/FurbySoup/quell/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Windows](https://img.shields.io/badge/platform-Windows-0078D4?logo=windows)](https://github.com/FurbySoup/quell/releases)
[![Rust](https://img.shields.io/badge/built%20with-Rust-dea584?logo=rust)](https://www.rust-lang.org/)

**Windows-native terminal proxy that eliminates scroll-jumping for AI CLI tools.**

When Claude Code, Copilot CLI, or Gemini CLI stream long responses, your terminal's scroll position jumps to the top of the visible output on every update — making it impossible to read anything while new content arrives. quell sits between your terminal and the AI tool, keeping your scroll position stable.

## The Problem

Every AI CLI tool streams output through VT escape sequences. Terminals reset the scroll position to the top of the output on each update, causing constant scroll-jumping during long responses. This is the [#1 complaint](https://github.com/anthropics/claude-code/issues/1208) across AI CLI tools, with hundreds of upvotes across multiple issue trackers.

## How It Works

```
Your Terminal  <-->  quell (proxy)  <-->  ConPTY  <-->  AI CLI tool
```

quell intercepts the child process output via Windows ConPTY, processes VT escape sequences, filters dangerous sequences, and forwards clean output to your terminal. Your scroll position stays exactly where you left it.

## Features

- **Scroll stability** — read earlier output while new content streams in
- **Shift+Enter support** — inserts newline in Claude Code via [Kitty keyboard protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/) (Windows Terminal 1.25+)
- **Security filtering** — blocks clipboard access (OSC 52), dangerous URL schemes (ssh://, javascript://), terminal query attacks, and C1 control characters
- **Full Unicode** — emoji, CJK, box-drawing, mathematical symbols all render correctly
- **Tool-agnostic** — works with Claude Code, Copilot CLI, Gemini CLI, or any terminal program
- **Zero config** — just prefix your command with `quell`
- **No network, no telemetry** — the binary makes zero network connections

## Quick Start

### Install

1. Download `quell.exe` from [Releases](https://github.com/FurbySoup/quell/releases)
2. Place it in a folder on your PATH (e.g. `C:\Users\YOU\.local\bin`)
3. Open a new terminal and run:

```bash
quell claude
```

That's it. quell shows a banner when it starts, launches Claude Code behind it, and keeps your scroll position stable.

### Usage

```bash
# Run Claude Code through quell
quell claude -- --dangerously-skip-permissions

# Run any AI CLI tool
quell gemini
quell copilot

# Explicit tool override (affects Shift+Enter behavior)
quell --tool claude my-custom-claude-wrapper

# Verbose output for troubleshooting
quell --verbose claude
```

### Build from Source

Requires [Rust](https://rustup.rs/) (stable toolchain).

```bash
git clone https://github.com/FurbySoup/quell.git
cd quell
cargo build --release
# Binary at target/release/quell.exe
```

### Troubleshooting

**`quell claude` does nothing / not recognized**
Your PATH points to the exe file itself instead of the folder containing it. `PATH` entries must be directories, not files. Move `quell.exe` into a directory that's already on your PATH, or add the directory (not the file) to PATH.

**`failed to spawn process (0x80070002)`**
The child command (`claude`, `gemini`, etc.) isn't on your PATH. Run `where claude` to check. If nothing is found, install the tool first.

**Scroll still jumps**
Make sure you're on the latest release. Run `quell --verbose claude` and check the debug output for clues. If the issue persists, [open an issue](https://github.com/FurbySoup/quell/issues) with the verbose log.

## Configuration

quell works out of the box with no configuration. Optional settings can be placed in `%APPDATA%\quell\config.toml`:

```toml
render_delay_ms = 5        # Normal output coalescing (ms)
sync_delay_ms = 50         # Sync block coalescing (ms)
history_lines = 100000     # Scrollback buffer size
log_level = "info"         # trace, debug, info, warn, error
log_file = "C:\\logs\\quell.log"  # Optional — logs to stderr if omitted
```

CLI flags override config file values. See `quell --help` for all options.

## Security

AI-generated output is untrusted. quell classifies every VT escape sequence and blocks known attack vectors:

| Category | Action | Examples |
|----------|--------|----------|
| **Blocked** | Stripped entirely | Clipboard access (OSC 52), font queries (OSC 50), terminal device queries |
| **Filtered** | Sanitized before forwarding | Window titles (control chars stripped), hyperlinks (URL scheme whitelist) |
| **Allowed** | Passed through | Cursor movement, colors, screen management, sync markers |

The URL scheme whitelist allows `http`, `https`, and `file` only — blocking schemes used in real CVEs ([CVE-2023-46321](https://nvd.nist.gov/vuln/detail/CVE-2023-46321), [CVE-2023-46322](https://nvd.nist.gov/vuln/detail/CVE-2023-46322)).

See [SECURITY.md](SECURITY.md) for the full threat model.

## Requirements

- **Windows 10 1809+** (ConPTY support required)
- **Windows Terminal 1.25+** for Shift+Enter support (older terminals still work, Alt+Enter remains available)

## Known Limitations

- **Emoji picker (WIN+.)** and **IME input** may not work through quell. This is a ConPTY limitation. Workaround: copy-paste emoji via Ctrl+V.

## Roadmap

- **Phase 1: CLI proxy** — scroll stability, security filtering, Shift+Enter, startup banner, friendly errors, `--verbose` diagnostics
- **Phase 2:** Standalone terminal (Tauri + xterm.js) with structured output, collapsible sections, tabs, accessibility
- **Phase 3:** Session persistence, split panes, search, community release

## License

[MIT](LICENSE)
