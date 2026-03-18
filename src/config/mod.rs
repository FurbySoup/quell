mod settings;
mod tool;

pub use settings::AppConfig;
pub use tool::ToolKind;

use clap::Parser;

/// Quell — Windows-native terminal proxy for AI CLI tools
///
/// Eliminates scroll-jumping and flicker by intercepting VT output,
/// tracking screen state, and sending only differential updates.
#[derive(Parser, Debug)]
#[command(name = "quell", version, about)]
pub struct Cli {
    /// Command to run (defaults to "claude")
    #[arg(value_name = "COMMAND")]
    pub command: Option<String>,

    /// Arguments to pass to the child command
    #[arg(trailing_var_arg = true)]
    pub args: Vec<String>,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info", env = "RUST_LOG")]
    pub log_level: String,

    /// Log file path (if not set, logs to stderr)
    #[arg(long, env = "QUELL_LOG_FILE")]
    pub log_file: Option<String>,

    /// Config file path (defaults to %APPDATA%\quell\config.toml)
    #[arg(long, short)]
    pub config: Option<String>,

    /// Render delay in milliseconds for normal output
    #[arg(long, default_value = "5")]
    pub render_delay_ms: u64,

    /// Render delay in milliseconds for synchronized output blocks
    #[arg(long, default_value = "50")]
    pub sync_delay_ms: u64,

    /// Maximum history lines to retain
    #[arg(long, default_value = "100000")]
    pub history_lines: usize,

    /// AI tool override (auto-detected from command if not set)
    #[arg(long, value_parser = tool::parse_tool_kind)]
    pub tool: Option<ToolKind>,
}
