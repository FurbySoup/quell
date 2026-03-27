# Security & Trust Model

## What This Tool Does

quell is a terminal proxy that sits between your terminal emulator and an AI CLI tool (Claude Code, Copilot CLI, Gemini CLI). It intercepts VT output, tracks screen state, and sends only differential updates to eliminate scroll-jumping and flicker.

## What The Proxy Can See

The proxy processes **all terminal output** from the child process. This includes:
- AI-generated code and responses
- File contents displayed by the AI tool
- Error messages (which may contain paths, stack traces)
- Environment variable dumps (if displayed by the tool)
- API keys or secrets if they appear in terminal output

The proxy also processes **all keyboard input** from the user, forwarding it to the child process.

## What The Proxy Does NOT Do

- **No network access.** The proxy binary makes zero network connections. It communicates only with the child process (via ConPTY pipes) and the user's terminal (via stdin/stdout). You can verify this yourself with `netstat -b` or Sysinternals Process Monitor.
- **No content logging.** Output content is never written to log files at default log levels (`info`, `debug`). These levels capture only metadata: frame counts, byte throughput, render timing, errors. The `trace` level may include raw VT bytes and will warn you at startup if enabled.
- **No telemetry.** No usage data is collected or transmitted. Ever.
- **No data persistence.** Terminal content exists only in memory (the history buffer) and is discarded when the proxy exits.

## Security Measures

### Escape Sequence Filtering

AI-generated output is untrusted content. The proxy classifies VT escape sequences and blocks known attack vectors:

**Blocked sequences** (never forwarded to your terminal):
- OSC 52 (clipboard read/write) — prevents child process from accessing your clipboard
- OSC 50 (font query) — prevents font name echo attacks
- C1 control characters (U+0080-U+009F, UTF-8 encoded) — prevents ambiguous interpretation attacks

**Filtered sequences** (forwarded with sanitization):
- OSC 8 hyperlinks — URL scheme whitelist (`http`, `https`, `file` only). Schemes used in real CVEs (`ssh://`, `x-man-page://`) are stripped.
- OSC 2 window title — control characters stripped from title text

**Allowed sequences** (safe, forwarded as-is):
- Cursor movement, text styling (SGR), screen management, DEC mode 2026 sync markers, scrolling

### No Terminal Query Responses

The proxy never responds to terminal query sequences (DA, DECRQSS, title report) on behalf of itself. This prevents echoback attacks — the most common terminal vulnerability class — where attacker-controlled data is injected as terminal input.

### Config File Safety

The proxy does not execute commands or shell expansions from configuration values. Configuration is limited to declarative settings (numeric values, string identifiers, lists).

### Spawn Parameter Validation (GUI)

Extra arguments passed to the shell spawn command are validated to contain only CLI flags (tokens starting with `-`) free of shell metacharacters. Working directory paths are validated to be local, absolute, existing directories — UNC paths are rejected to prevent implicit NTLM credential leaks.

### Raw Mode Restoration

The proxy saves and restores terminal mode on exit, including on panic. A crash cannot leave the terminal in an unusable state.

## Threat Model

### Trusted
- The user's terminal emulator (it already sees everything)
- The user's configuration files in `%APPDATA%`
- The Rust standard library and direct dependencies

### Untrusted
- All output from the child process (AI tools may operate on malicious repositories)
- OSC 8 hyperlinks in child output (display text may differ from actual URL)
- Project-local configuration files (could be placed by a malicious repository)

### Attack Scenarios Considered

| Scenario | Mitigation |
|----------|------------|
| Malicious child emits crafted escape sequences to attack outer terminal | Escape sequence filtering (block/filter/allow classification) |
| Child emits OSC 8 link with spoofed display text | URL scheme whitelist + mismatch detection (Phase 2) |
| Child attempts clipboard read via OSC 52 | OSC 52 stripped entirely |
| Supply chain compromise of a dependency | `cargo-audit` in CI, minimal dependency surface |
| Modified binary distributed as the real tool | SHA256 checksums on releases, code signing (Phase 2) |
| Malicious `.quell.toml` in a cloned repo | Warning on project-local config, no command execution from config |
| Trace logging captures secrets to log file | Startup warning when trace enabled; default level captures metadata only |
| GUI parameter injection via IPC | Spawn arguments validated (flags only, no metacharacters); CWD validated (local absolute directory only) |

## Vulnerability Reporting

If you discover a security vulnerability, please report it through [GitHub's Private Vulnerability Reporting](https://github.com/FurbySoup/quell/security/advisories/new) rather than opening a public issue. We will acknowledge receipt within 48 hours and aim to release a fix within 7 days for critical issues.

## Verification

Users can verify the proxy's security claims:

**No network access:**
```
# While the proxy is running:
netstat -b | findstr quell
# Should return nothing
```

**No content in logs:**
```
# Run with default log level, then inspect:
findstr /i "api_key\|password\|secret" logs\quell.log
# Should return nothing (only metadata is logged)
```

**Dependency audit:**
```
cargo audit
cargo deny check
```

## Licensing

MIT license — free for any use, commercial or personal. See LICENSE file.
