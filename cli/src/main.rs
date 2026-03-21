// Phase 1 CLI binary — the quell terminal proxy.
//
// Uses the quell library crate for all engine functionality.
// This file contains only CLI-specific concerns: arg parsing,
// logging setup, banner, friendly errors, and the run loop.

use anyhow::{Context, Result};
use clap::Parser;
use tracing::{info, warn};

use quell::config::{self, AppConfig, ConfigOverrides};
use quell::conpty::{self, ConsoleMode, ConPtySession};
use quell::proxy::Proxy;

/// Quell — Windows-native terminal proxy for AI CLI tools
///
/// Eliminates scroll-jumping and flicker by intercepting VT output,
/// tracking screen state, and sending only differential updates.
#[derive(Parser, Debug)]
#[command(name = "quell", version, about)]
struct Cli {
    /// Command to run (defaults to "claude")
    #[arg(value_name = "COMMAND")]
    command: Option<String>,

    /// Arguments to pass to the child command
    #[arg(trailing_var_arg = true)]
    args: Vec<String>,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, env = "RUST_LOG")]
    log_level: Option<String>,

    /// Log file path (if not set, logs to stderr)
    #[arg(long, env = "QUELL_LOG_FILE")]
    log_file: Option<String>,

    /// Config file path (defaults to %APPDATA%\quell\config.toml)
    #[arg(long, short)]
    config: Option<String>,

    /// Render delay in milliseconds for normal output
    #[arg(long)]
    render_delay_ms: Option<u64>,

    /// Render delay in milliseconds for synchronized output blocks
    #[arg(long)]
    sync_delay_ms: Option<u64>,

    /// Maximum history lines to retain
    #[arg(long)]
    history_lines: Option<usize>,

    /// AI tool override (auto-detected from command if not set)
    #[arg(long, value_parser = config::parse_tool_kind)]
    tool: Option<config::ToolKind>,

    /// Enable verbose output for troubleshooting
    #[arg(short, long)]
    verbose: bool,

    /// Record filtered VT output to a .vtcap file for replay testing
    #[cfg(feature = "recording")]
    #[arg(long, value_name = "PATH")]
    record: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging before anything else
    let _guard = init_logging(&cli)?;

    info!(
        version = env!("CARGO_PKG_VERSION"),
        "quell starting"
    );

    // Warn if trace-level logging is enabled — it may capture raw terminal content
    if tracing::enabled!(tracing::Level::TRACE) {
        warn!("TRACE logging is enabled — raw terminal content may appear in logs. \
               This should only be used for debugging, never in production.");
    }

    // Load configuration
    let overrides = ConfigOverrides {
        config_path: cli.config.clone(),
        render_delay_ms: cli.render_delay_ms,
        sync_delay_ms: cli.sync_delay_ms,
        history_lines: cli.history_lines,
        log_level: cli.log_level.clone(),
        log_file: cli.log_file.clone(),
    };
    let config = AppConfig::load(&overrides).context("failed to load configuration")?;
    info!(?config, "configuration loaded");

    // Determine the child command to run
    let child_command = cli.command.clone().unwrap_or_else(|| "claude".to_string());
    let child_args: Vec<String> = cli.args.clone();

    // Build full command line
    let command_line = if child_args.is_empty() {
        child_command.clone()
    } else {
        format!("{} {}", child_command, child_args.join(" "))
    };

    // Detect AI tool: CLI flag overrides auto-detection from command
    let tool = cli.tool.unwrap_or_else(|| config::ToolKind::detect(&command_line));
    info!(
        command = %command_line,
        tool = %tool,
        "launching child process"
    );

    // Print startup banner to stderr so it's always visible
    print_banner(&child_command);

    // Run the proxy — returns the child's exit code
    #[cfg(feature = "recording")]
    let exit_code = run_proxy(&command_line, config, tool, cli.record.as_deref())?;
    #[cfg(not(feature = "recording"))]
    let exit_code = run_proxy(&command_line, config, tool)?;

    info!(exit_code, "quell shutting down");

    // Force-exit the process. The input thread may still be blocked in
    // ReadFile on the console handle (Windows doesn't support cancelling
    // console reads reliably). Without this, the process hangs after
    // main() returns because Rust's runtime waits for all threads.
    std::process::exit(exit_code as i32);
}

#[cfg(feature = "recording")]
fn run_proxy(command_line: &str, config: AppConfig, tool: config::ToolKind, record_path: Option<&str>) -> Result<u32> {
    let (cols, rows, console_mode, session) = setup_proxy(command_line)?;

    // Create and run the proxy
    let (proxy, _events) = Proxy::new(config, tool, session);
    let proxy = if let Some(path) = record_path {
        let recorder = quell::proxy::recorder::VtcapRecorder::create(
            std::path::Path::new(path),
            cols as u16,
            rows as u16,
            &"quell",
        )?;
        proxy.with_recorder(recorder)
    } else {
        proxy
    };
    let result = proxy.run();

    teardown_proxy(console_mode);
    result
}

#[cfg(not(feature = "recording"))]
fn run_proxy(command_line: &str, config: AppConfig, tool: config::ToolKind) -> Result<u32> {
    let (_cols, _rows, console_mode, session) = setup_proxy(command_line)?;

    // Create and run the proxy
    let (proxy, _events) = Proxy::new(config, tool, session);
    let result = proxy.run();

    teardown_proxy(console_mode);
    result
}

/// Common proxy setup: detect size, set console mode, spawn child.
fn setup_proxy(command_line: &str) -> Result<(i16, i16, ConsoleMode, ConPtySession)> {
    let (cols, rows) = conpty::get_terminal_size().unwrap_or((120, 30));
    info!(cols, rows, "detected terminal size");

    let console_mode = ConsoleMode::save_and_set_raw()
        .context("failed to save/set console mode")?;

    install_panic_hook();

    let session = match ConPtySession::spawn(command_line, cols, rows) {
        Ok(s) => s,
        Err(e) => {
            let _ = console_mode.restore();
            std::mem::forget(console_mode);
            let cmd_name = command_line.split_whitespace().next().unwrap_or(command_line);
            let wrapped = anyhow::Error::from(e).context("failed to create ConPTY session");
            if print_friendly_spawn_error(cmd_name, &wrapped) {
                tracing::debug!(error = %wrapped, "spawn failed");
                std::process::exit(1);
            }
            return Err(wrapped);
        }
    };

    Ok((cols, rows, console_mode, session))
}

/// Common proxy teardown: restore console mode.
fn teardown_proxy(console_mode: ConsoleMode) {
    if let Err(e) = console_mode.restore() {
        warn!(error = %e, "failed to restore console mode");
    }
    std::mem::forget(console_mode);
}

/// Print a branded startup banner to stderr.
fn print_banner(command: &str) {
    let version = env!("CARGO_PKG_VERSION");
    eprintln!(" \u{250f}\u{2501}\u{2501} quell \u{2501}\u{2501}\u{2513}");
    eprintln!(" \u{2503}  v{version:<6}\u{2503}  launching '{command}' \u{00b7} scroll-fix: active");
    eprintln!(" \u{2517}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{251b}");
}

/// Print a friendly error message for known Windows spawn failures.
fn print_friendly_spawn_error(command: &str, error: &anyhow::Error) -> bool {
    for cause in error.chain() {
        if let Some(win_err) = cause.downcast_ref::<windows::core::Error>() {
            let code = win_err.code().0 as u32;
            match code {
                0x80070002 => {
                    eprintln!("error: '{command}' not found.");
                    eprintln!("  Make sure it's installed and on your PATH.");
                    eprintln!("  Run 'where {command}' to check.");
                    return true;
                }
                0x80070005 => {
                    eprintln!("error: Permission denied when launching '{command}'.");
                    eprintln!("  Try running as administrator.");
                    return true;
                }
                _ => {}
            }
        }
    }
    false
}

/// Install a panic hook that restores console mode before printing the panic.
fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        ConsoleMode::emergency_restore();
        default_hook(info);
    }));
}

fn init_logging(cli: &Cli) -> Result<Option<tracing_appender::non_blocking::WorkerGuard>> {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};

    let log_level = cli.log_level.as_deref().unwrap_or("info");

    let effective_log_file = if cli.verbose && cli.log_file.is_none() {
        let path = std::env::temp_dir().join("quell-verbose.log");
        eprintln!("  verbose: logging to {}", path.display());
        Some(path.to_string_lossy().into_owned())
    } else {
        cli.log_file.clone()
    };

    let effective_level = if cli.verbose { "debug" } else { log_level };

    if let Some(log_file) = &effective_log_file {
        let log_dir = std::path::Path::new(log_file)
            .parent()
            .unwrap_or(std::path::Path::new("."));
        let log_filename = std::path::Path::new(log_file)
            .file_name()
            .unwrap_or(std::ffi::OsStr::new("quell.log"));

        let file_filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(effective_level));

        let file_appender = tracing_appender::rolling::daily(log_dir, log_filename);
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

        tracing_subscriber::registry()
            .with(file_filter)
            .with(
                fmt::layer()
                    .with_writer(non_blocking)
                    .with_ansi(false)
                    .with_target(true)
                    .with_thread_ids(true)
                    .with_line_number(true),
            )
            .init();

        Ok(Some(guard))
    } else {
        let stderr_filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| {
                if cli.log_level.is_none() {
                    EnvFilter::new("warn")
                } else {
                    EnvFilter::new(log_level)
                }
            });

        tracing_subscriber::registry()
            .with(stderr_filter)
            .with(
                fmt::layer()
                    .with_writer(std::io::stderr)
                    .with_target(true),
            )
            .init();

        Ok(None)
    }
}
