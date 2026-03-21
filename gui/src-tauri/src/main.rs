#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;
use std::thread;

use base64::Engine;
use crossbeam_channel::{Receiver, Sender};
use serde::Serialize;
use tauri::{Emitter, Manager};
use tracing::{error, info, warn};

/// Wrapper to send Proxy across thread boundaries.
/// Safe because the Proxy is used exclusively on the spawned thread —
/// ConPTY handles are valid from any single thread, just not Send by default
/// because HANDLE wraps a raw pointer.
struct SendProxy(quell::proxy::Proxy);
unsafe impl Send for SendProxy {}

use quell::config::{AppConfig, ToolKind};
use quell::conpty::ConPtySession;
use quell::proxy::output_sink::ChannelSink;
use quell::proxy::Proxy;

/// Shared state: channels for sending input/resize to the proxy's external I/O.
struct ProxyState {
    input_tx: Sender<Vec<u8>>,
    resize_tx: Sender<(i16, i16)>,
}

/// Tauri-managed state wrapping the proxy channels.
/// None until spawn_shell is called.
struct AppState {
    proxy: Mutex<Option<ProxyState>>,
}

/// Event payload sent to the frontend with terminal output.
#[derive(Clone, Serialize)]
struct TerminalOutput {
    /// Base64-encoded terminal output bytes
    data: String,
}

/// Event payload sent when the child process exits.
#[derive(Clone, Serialize)]
struct ChildExited {
    exit_code: u32,
}

/// Spawn a shell process and wire it through the quell proxy.
/// The command defaults to "cmd.exe" if not provided.
#[tauri::command]
fn spawn_shell(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    command: Option<String>,
    cols: Option<i16>,
    rows: Option<i16>,
) -> Result<(), String> {
    let mut proxy_guard = state.proxy.lock().map_err(|e| e.to_string())?;
    if proxy_guard.is_some() {
        return Err("shell already spawned".into());
    }

    let command = command.unwrap_or_else(|| "cmd.exe".into());
    let cols = cols.unwrap_or(120);
    let rows = rows.unwrap_or(30);

    let tool = ToolKind::detect(&command);
    info!(command = %command, cols, rows, tool = %tool, "spawning shell for GUI");

    let session = ConPtySession::spawn(&command, cols, rows)
        .map_err(|e| format!("failed to spawn: {e}"))?;

    let config = AppConfig::default();

    // Create channels for external I/O (GUI mode)
    let (input_tx, input_rx): (Sender<Vec<u8>>, Receiver<Vec<u8>>) =
        crossbeam_channel::bounded(64);
    let (resize_tx, resize_rx) = crossbeam_channel::bounded::<(i16, i16)>(4);

    // Create the channel-based output sink
    let (sink, output_rx) = ChannelSink::new();

    // Build the proxy with external I/O
    let (proxy, event_rx) = Proxy::new(config, tool, session, Box::new(sink));
    let proxy = proxy.with_external_io(input_rx, resize_rx);

    // Store channels so commands can send input/resize
    *proxy_guard = Some(ProxyState {
        input_tx,
        resize_tx,
    });
    drop(proxy_guard);

    // Output forwarder thread: reads from ChannelSink, emits Tauri events
    let app_handle = app.clone();
    thread::Builder::new()
        .name("output-forwarder".into())
        .spawn(move || {
            let b64 = base64::engine::general_purpose::STANDARD;
            while let Ok(data) = output_rx.recv() {
                let encoded = b64.encode(&data);
                if let Err(e) = app_handle.emit("terminal-output", TerminalOutput { data: encoded }) {
                    warn!(error = %e, "failed to emit terminal-output event");
                    break;
                }
            }
            info!("output forwarder exiting");
        })
        .ok();

    // Proxy event forwarder: watches for ChildExited
    let app_handle2 = app.clone();
    thread::Builder::new()
        .name("event-forwarder".into())
        .spawn(move || {
            while let Ok(event) = event_rx.recv() {
                if let quell::proxy::events::ProxyEvent::ChildExited { exit_code } = event {
                    info!(exit_code, "child exited, emitting event");
                    let _ = app_handle2.emit("child-exited", ChildExited { exit_code });
                    break;
                }
            }
        })
        .ok();

    // Proxy runner thread: runs the blocking proxy loop.
    // Safety: ConPTY HANDLE is not Send because it wraps *mut c_void, but
    // the proxy is constructed here and moved exclusively to this thread.
    // No other thread accesses the session handles.
    let send_proxy = SendProxy(proxy);
    thread::Builder::new()
        .name("proxy-runner".into())
        .spawn(move || {
            let proxy = send_proxy;
            match proxy.0.run() {
                Ok(code) => info!(exit_code = code, "proxy finished"),
                Err(e) => error!(error = %e, "proxy error"),
            }
        })
        .ok();

    info!("shell spawned successfully");
    Ok(())
}

/// Write input data to the shell (from xterm.js onData).
/// Data is a plain string (keyboard input from xterm.js).
#[tauri::command]
fn write_input(
    state: tauri::State<'_, AppState>,
    data: String,
) -> Result<(), String> {
    let proxy_guard = state.proxy.lock().map_err(|e| e.to_string())?;
    let proxy_state = proxy_guard.as_ref().ok_or("no shell running")?;
    proxy_state
        .input_tx
        .send(data.into_bytes())
        .map_err(|_| "input channel closed".to_string())
}

/// Resize the PTY (from xterm.js fit addon).
#[tauri::command]
fn resize_pty(
    state: tauri::State<'_, AppState>,
    cols: i16,
    rows: i16,
) -> Result<(), String> {
    let proxy_guard = state.proxy.lock().map_err(|e| e.to_string())?;
    let proxy_state = proxy_guard.as_ref().ok_or("no shell running")?;
    proxy_state
        .resize_tx
        .try_send((cols, rows))
        .map_err(|e| format!("resize channel error: {e}"))
}

/// Detect Windows dark/light mode from the registry.
/// Returns "dark" or "light".
#[tauri::command]
fn get_system_theme() -> String {
    use windows::Win32::System::Registry::{
        HKEY_CURRENT_USER, REG_DWORD, RegGetValueW, RRF_RT_DWORD,
    };
    use windows::core::w;

    let mut data: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    let mut kind = REG_DWORD;

    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_DWORD,
            Some(&mut kind),
            Some(&mut data as *mut u32 as *mut std::ffi::c_void),
            Some(&mut size),
        )
    };

    if result.is_ok() && data == 1 {
        "light".to_string()
    } else {
        // Default to dark if registry read fails or value is 0
        "dark".to_string()
    }
}

/// Return the default command from AppConfig (loaded from config file).
#[tauri::command]
fn get_default_command() -> String {
    let config = AppConfig::load(&Default::default()).unwrap_or_default();
    config.default_command
}

fn main() {
    // Initialize tracing to a log file for GUI debugging
    let log_dir = std::env::temp_dir();
    let file_appender = tracing_appender::rolling::daily(&log_dir, "quell-gui.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(true)
        .with_env_filter("info")
        .init();

    info!(version = env!("CARGO_PKG_VERSION"), "quell GUI starting");

    tauri::Builder::default()
        .manage(AppState {
            proxy: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            spawn_shell,
            write_input,
            resize_pty,
            get_system_theme,
            get_default_command,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                info!("window close requested, cleaning up");
                // Drop the proxy channels to signal shutdown.
                // The input_tx drop causes the proxy's input loop to end,
                // which triggers ConPTY session cleanup and child termination.
                if let Some(state) = window.try_state::<AppState>() {
                    let mut guard = state.proxy.lock().unwrap_or_else(|e: std::sync::PoisonError<_>| e.into_inner());
                    let _ = guard.take();
                    info!("proxy state cleared");
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error running Quell GUI");
}
