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

### Download

Grab the latest `quell.exe` from [Releases](https://github.com/FurbySoup/quell/releases).

### Usage

```bash
# Run Claude Code through quell
quell claude -- --dangerously-skip-permissions

# Run any AI CLI tool
quell gemini
quell copilot

# Explicit tool override (affects Shift+Enter behavior)
quell --tool claude my-custom-claude-wrapper
```

### Build from Source

Requires [Rust](https://rustup.rs/) (stable toolchain).

```bash
git clone https://github.com/FurbySoup/quell.git
cd quell
cargo build --release
# Binary at target/release/quell.exe
```

## Configuration

quell works out of the box with no configuration. Optional settings can be placed in `%APPDATA%\quell\config.toml`:

```toml
[history]
max_lines = 100_000        # Scrollback buffer size

[logging]
level = "info"             # error, warn, info, debug, trace
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

## Roadmap

quell is currently a CLI proxy (Phase 1). Future phases:

- **Phase 2:** Standalone terminal (Tauri + xterm.js) with structured output, collapsible sections, tabs, accessibility
- **Phase 3:** Session persistence, split panes, search, community release

## License

[MIT](LICENSE)
