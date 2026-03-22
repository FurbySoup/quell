#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashMap;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

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
use quell::proxy::render_coalescer::RenderCoalescer;

/// Per-session state: channels for sending input/resize to the proxy's external I/O.
struct SessionState {
    input_tx: Sender<Vec<u8>>,
    resize_tx: Sender<(i16, i16)>,
}

/// Tauri-managed state wrapping all sessions.
struct AppState {
    sessions: Mutex<HashMap<String, SessionState>>,
    next_id: Mutex<u32>,
}

impl AppState {
    fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            next_id: Mutex::new(1),
        }
    }

    fn next_session_id(&self) -> String {
        let mut id = self.next_id.lock().unwrap_or_else(|e| e.into_inner());
        let session_id = format!("session-{id}");
        *id += 1;
        session_id
    }
}

/// Event payload sent to the frontend with terminal output.
#[derive(Clone, Serialize)]
struct TerminalOutput {
    /// Session this output belongs to
    session_id: String,
    /// Base64-encoded terminal output bytes
    data: String,
}

/// Event payload sent when the child process exits.
#[derive(Clone, Serialize)]
struct ChildExited {
    session_id: String,
    exit_code: u32,
}

/// Spawn a shell process and wire it through the quell proxy.
/// Returns the session_id for the new session.
#[tauri::command]
fn spawn_shell(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    command: Option<String>,
    cols: Option<i16>,
    rows: Option<i16>,
    session_id: Option<String>,
) -> Result<String, String> {
    let session_id = session_id.unwrap_or_else(|| state.next_session_id());

    let command = command.unwrap_or_else(|| "cmd.exe".into());
    let cols = cols.unwrap_or(120);
    let rows = rows.unwrap_or(30);

    let tool = ToolKind::detect(&command);
    info!(session_id = %session_id, command = %command, cols, rows, tool = %tool, "spawning shell for GUI");

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

    // Store session state
    {
        let mut sessions = state.sessions.lock().map_err(|e| e.to_string())?;
        sessions.insert(session_id.clone(), SessionState {
            input_tx,
            resize_tx,
        });
    }

    // Output forwarder thread: raw passthrough with RenderCoalescer for batching.
    // Raw VT bytes flow directly to xterm.js, preserving scrollback.
    //
    // Resize-during-streaming may cause visual artifacts from ConPTY's
    // cursor-positioned redraw — this is a known ConPTY limitation
    // (microsoft/terminal#14774), same as the Phase 1 CLI.
    let app_handle = app.clone();
    let sid = session_id.clone();
    thread::Builder::new()
        .name(format!("output-forwarder-{sid}"))
        .spawn(move || {
            let b64 = base64::engine::general_purpose::STANDARD;
            let mut coalescer = RenderCoalescer::new(
                Duration::from_millis(5),   // render_delay: batch rapid output
                Duration::from_millis(50),  // sync_delay: wait after sync blocks
                Duration::from_millis(16),  // min_frame_time: ~60fps cap
            );
            let mut pending_output: Vec<u8> = Vec::new();

            let sid_ref = &sid;
            let emit = |data: &[u8]| -> bool {
                let encoded = b64.encode(data);
                if let Err(e) = app_handle.emit(
                    "terminal-output",
                    TerminalOutput {
                        session_id: sid_ref.clone(),
                        data: encoded,
                    },
                ) {
                    warn!(error = %e, session_id = %sid_ref, "failed to emit terminal-output event");
                    return false;
                }
                true
            };

            loop {
                let timeout = coalescer.time_until_render()
                    .unwrap_or(Duration::from_secs(60));

                crossbeam_channel::select! {
                    recv(output_rx) -> msg => {
                        match msg {
                            Ok(data) => {
                                pending_output.extend_from_slice(&data);
                                coalescer.notify_data();
                                // Drain any additional buffered chunks
                                while let Ok(more) = output_rx.try_recv() {
                                    pending_output.extend_from_slice(&more);
                                }
                            }
                            Err(_) => {
                                // Channel closed — flush and exit
                                if !pending_output.is_empty() {
                                    emit(&pending_output);
                                }
                                break;
                            }
                        }
                    }
                    default(timeout) => {
                        // Timer fired
                    }
                }

                // Emit when coalescer says it's time
                if coalescer.should_render() && !pending_output.is_empty() {
                    if !emit(&pending_output) {
                        break;
                    }
                    pending_output.clear();
                    coalescer.mark_rendered();
                }
            }
            info!(session_id = %sid, "output forwarder exiting");
        })
        .ok();

    // Proxy event forwarder: watches for ChildExited, clears session state for re-spawn
    let app_handle2 = app.clone();
    let sid = session_id.clone();
    thread::Builder::new()
        .name(format!("event-forwarder-{sid}"))
        .spawn(move || {
            while let Ok(event) = event_rx.recv() {
                if let quell::proxy::events::ProxyEvent::ChildExited { exit_code } = event {
                    info!(exit_code, session_id = %sid, "child exited, emitting event");
                    // Clear this session's state so it can be re-spawned
                    if let Some(state) = app_handle2.try_state::<AppState>() {
                        let mut sessions = state.sessions.lock()
                            .unwrap_or_else(|e| e.into_inner());
                        sessions.remove(&sid);
                        info!(session_id = %sid, "session state cleared for re-spawn");
                    }
                    let _ = app_handle2.emit("child-exited", ChildExited {
                        session_id: sid.clone(),
                        exit_code,
                    });
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
    let sid = session_id.clone();
    thread::Builder::new()
        .name(format!("proxy-runner-{sid}"))
        .spawn(move || {
            let proxy = send_proxy;
            match proxy.0.run() {
                Ok(code) => info!(exit_code = code, session_id = %sid, "proxy finished"),
                Err(e) => error!(error = %e, session_id = %sid, "proxy error"),
            }
        })
        .ok();

    info!(session_id = %session_id, "shell spawned successfully");
    Ok(session_id)
}

/// Write input data to the shell (from xterm.js onData).
/// Data is base64-encoded keyboard input from xterm.js.
#[tauri::command]
fn write_input(
    state: tauri::State<'_, AppState>,
    session_id: String,
    data: String,
) -> Result<(), String> {
    use base64::engine::general_purpose::STANDARD;
    let decoded = STANDARD.decode(&data).map_err(|e| format!("base64 decode failed: {e}"))?;
    let sessions = state.sessions.lock().map_err(|e| e.to_string())?;
    let session = sessions.get(&session_id).ok_or("no such session")?;
    session
        .input_tx
        .send(decoded)
        .map_err(|_| "input channel closed".to_string())
}

/// Resize the PTY (from xterm.js fit addon).
#[tauri::command]
fn resize_pty(
    state: tauri::State<'_, AppState>,
    session_id: String,
    cols: i16,
    rows: i16,
) -> Result<(), String> {
    let sessions = state.sessions.lock().map_err(|e| e.to_string())?;
    let session = sessions.get(&session_id).ok_or("no such session")?;
    session
        .resize_tx
        .try_send((cols, rows))
        .map_err(|e| format!("resize channel error: {e}"))
}

/// Close a session by dropping its channels.
#[tauri::command]
fn close_session(
    state: tauri::State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    let mut sessions = state.sessions.lock().map_err(|e| e.to_string())?;
    if sessions.remove(&session_id).is_some() {
        info!(session_id = %session_id, "session closed by user");
        Ok(())
    } else {
        Err("no such session".into())
    }
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

    for value_name in [w!("SystemUsesLightTheme"), w!("AppsUseLightTheme")] {
        let result = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
                value_name,
                RRF_RT_DWORD,
                Some(&mut kind),
                Some(&mut data as *mut u32 as *mut std::ffi::c_void),
                Some(&mut size),
            )
        };
        if result.is_ok() {
            return if data == 1 { "light".to_string() } else { "dark".to_string() };
        }
    }
    "dark".to_string()
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
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            spawn_shell,
            write_input,
            resize_pty,
            close_session,
            get_system_theme,
            get_default_command,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                info!("window close requested, cleaning up");
                if let Some(state) = window.try_state::<AppState>() {
                    let mut sessions = state.sessions.lock()
                        .unwrap_or_else(|e: std::sync::PoisonError<_>| e.into_inner());
                    sessions.clear();
                    info!("all sessions cleared");
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error running Quell GUI");
}
