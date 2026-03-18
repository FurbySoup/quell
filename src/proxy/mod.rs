// Proxy module — the main event loop
//
// Coordinates:
// - ConPTY I/O threads (input + output)
// - Sync block detection
// - History management
//
// Architecture:
//   Input thread:  Real stdin → ConPTY input pipe (+ resize events → main thread)
//   Output thread: ConPTY output pipe → channel → main thread
//   Watcher thread: WaitForSingleObject on child process → shutdown signal
//   Main thread:   Sync detector → stdout passthrough + history + metrics
//
// Phase 1 strategy: transparent passthrough with instrumentation.
// All child output goes directly to stdout. The sync detector and history
// still process data for metrics and future Phase 2 use. Differential
// rendering is deferred to Phase 2 (Tauri terminal) where we control
// the display surface.
//
// Shutdown sequence (critical for ConPTY):
//   1. Child process exits → watcher thread sends ChildExited signal
//   2. Main loop breaks
//   3. ClosePseudoConsole is called (via session Drop) — this closes the
//      output pipe. MUST happen while the output thread is still reading
//      (pipe read end open), otherwise ClosePseudoConsole deadlocks.
//   4. Output thread gets pipe EOF and exits
//   5. Input thread is signaled via shutdown event

pub mod events;
pub mod key_translator;
pub mod output_sink;
#[cfg(feature = "recording")]
#[deny(dead_code)]
pub mod recorder;
pub mod render_coalescer;

pub use output_sink::OutputSink;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use anyhow::{Context, Result};
use crossbeam_channel::{bounded, select, Receiver, Sender};
use tracing::{debug, error, info, trace, warn};

use crate::config::{AppConfig, ToolKind};
use crate::conpty::ConPtySession;
use crate::history::{HistoryEventType, LineBuffer, OutputFilter};
use crate::vt::{SyncBlockDetector, SyncEvent};

use events::{event_channel, ProxyEvent};
use key_translator::{KeyTranslator, KITTY_DISABLE, KITTY_ENABLE};

/// Reason a thread is signaling shutdown
#[derive(Debug, Clone)]
enum ShutdownReason {
    InputEof,
    OutputEof,
    ChildExited,
    IoError(#[allow(dead_code)] String),
}

/// The proxy coordinator. Owns all processing state and runs the main loop.
pub struct Proxy {
    config: AppConfig,
    tool: ToolKind,
    session: ConPtySession,
    event_tx: Sender<ProxyEvent>,
    #[cfg(feature = "recording")]
    recorder: Option<recorder::VtcapRecorder>,
}

impl Proxy {
    /// Create a new proxy. Returns (proxy, event_receiver).
    /// In Phase 1, the caller can drop the receiver immediately.
    pub fn new(config: AppConfig, tool: ToolKind, session: ConPtySession) -> (Self, Receiver<ProxyEvent>) {
        let (event_tx, event_rx) = event_channel();
        let (cols, rows) = session.size();
        info!(cols, rows, tool = %tool, "proxy created");
        (
            Self {
                config,
                tool,
                session,
                event_tx,
                #[cfg(feature = "recording")]
                recorder: None,
            },
            event_rx,
        )
    }

    /// Attach a VtcapRecorder to capture filtered output during the session.
    #[cfg(feature = "recording")]
    pub fn with_recorder(mut self, recorder: recorder::VtcapRecorder) -> Self {
        self.recorder = Some(recorder);
        self
    }

    /// Run the proxy. Blocks until the child exits or an error occurs.
    /// Returns the child's exit code.
    pub fn run(mut self) -> Result<u32> {
        // Take I/O handles from session
        let (input_write, output_read) = self
            .session
            .take_io()
            .context("failed to take I/O handles from session")?;

        // Create pipeline components
        let (cols, rows) = self.session.size();
        let mut output_filter = OutputFilter::new();
        let mut detector = SyncBlockDetector::new();
        let mut history = LineBuffer::new(self.config.history_lines);
        let mut total_bytes: u64 = 0;
        let mut chunk_count: u64 = 0;

        // Channels
        let (output_tx, output_rx): (Sender<Vec<u8>>, Receiver<Vec<u8>>) = bounded(64);
        let (shutdown_tx, shutdown_rx): (Sender<ShutdownReason>, Receiver<ShutdownReason>) =
            bounded(4);
        let (resize_tx, resize_rx) = bounded::<(i16, i16)>(4);

        let shutdown_flag = Arc::new(AtomicBool::new(false));

        // No Ctrl+C handler — with ENABLE_PROCESSED_INPUT disabled in console mode,
        // Ctrl+C arrives as byte 0x03 via stdin, which our input thread writes to
        // ConPTY's input pipe. ConPTY then generates CTRL_C_EVENT for the child.
        // This gives natural Ctrl+C forwarding without interception.

        // Child exit watcher thread: detects when the child process exits.
        // ConPTY does NOT close the output pipe when the child exits — you must
        // call ClosePseudoConsole to break the pipe. This thread signals the main
        // loop so we can initiate the correct shutdown sequence.
        let child_process_raw = self.session.process_handle_raw();
        let watcher_shutdown_tx = shutdown_tx.clone();
        thread::Builder::new()
            .name("child-watcher".into())
            .spawn(move || {
                use windows::Win32::Foundation::HANDLE;
                use windows::Win32::System::Threading::{WaitForSingleObject, INFINITE};

                let handle = HANDLE(child_process_raw as *mut _);
                unsafe {
                    WaitForSingleObject(handle, INFINITE);
                }
                info!("child process exited (watcher)");
                let _ = watcher_shutdown_tx.try_send(ShutdownReason::ChildExited);
            })
            .context("failed to spawn child watcher thread")?;
        info!("child watcher thread started");

        // Output thread: reads from ConPTY output pipe, sends to main thread
        let output_shutdown_tx = shutdown_tx.clone();
        let output_flag = shutdown_flag.clone();
        let output_thread = thread::Builder::new()
            .name("conpty-output".into())
            .spawn(move || {
                let mut buf = vec![0u8; 8192];
                loop {
                    if output_flag.load(Ordering::Relaxed) {
                        break;
                    }
                    match output_read.read(&mut buf) {
                        Ok(0) => {
                            info!("output pipe EOF");
                            let _ = output_shutdown_tx.try_send(ShutdownReason::OutputEof);
                            break;
                        }
                        Ok(n) => {
                            debug!(bytes = n, "output chunk received");
                            if output_tx.send(buf[..n as usize].to_vec()).is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            if !output_flag.load(Ordering::Relaxed) {
                                warn!(error = %e, "output pipe read error");
                                let _ = output_shutdown_tx
                                    .try_send(ShutdownReason::IoError(e.to_string()));
                            }
                            break;
                        }
                    }
                }
            })
            .context("failed to spawn output thread")?;
        info!("output thread started");

        // Input thread: reads from real stdin, writes to ConPTY input pipe.
        //
        // When stdin is a real console, we use ReadConsoleInputW to get both
        // keyboard events AND resize events (WINDOW_BUFFER_SIZE_EVENT). This
        // gives instant resize response instead of the old 100ms polling.
        // WaitForMultipleObjects provides clean shutdown via a shutdown event.
        //
        // When stdin is a pipe (e.g. in tests), we fall back to blocking read
        // since the thread will unblock when the pipe closes.
        let input_shutdown_tx = shutdown_tx.clone();
        let input_flag = shutdown_flag.clone();

        // Create a manual-reset event to signal the input thread to shut down
        let shutdown_event = create_shutdown_event()
            .context("failed to create shutdown event")?;
        let shutdown_event_handle = shutdown_event;

        let stdin_is_console = is_stdin_console();
        debug!(stdin_is_console, "input thread mode selected");

        let tool = self.tool;
        let input_thread = thread::Builder::new()
            .name("conpty-input".into())
            .spawn(move || {
                if stdin_is_console {
                    run_console_input_loop(
                        input_write,
                        input_flag,
                        input_shutdown_tx,
                        shutdown_event_handle,
                        resize_tx,
                        tool,
                    );
                } else {
                    run_pipe_input_loop(input_write, input_flag, input_shutdown_tx);
                }
            })
            .context("failed to spawn input thread")?;
        info!("input thread started");

        let stdout_handle = raw_stdout_handle();
        let mut last_size = (cols, rows);

        // Enable Kitty keyboard protocol on the outer terminal.
        // Terminals that don't support it will ignore the sequence.
        if let Err(e) = raw_write_all(stdout_handle, KITTY_ENABLE) {
            warn!(error = %e, "failed to send Kitty protocol enable");
        } else {
            info!("Kitty keyboard protocol enable sent");
        }

        info!("entering main proxy loop (passthrough mode)");

        loop {
            select! {
                recv(output_rx) -> msg => {
                    match msg {
                        Ok(data) => {
                            // Filter dangerous sequences before output
                            let filtered = output_filter.filter(&data);

                            // Record post-filter data for replay testing
                            #[cfg(feature = "recording")]
                            if let Some(ref mut rec) = self.recorder
                                && let Err(e) = rec.write_chunk(filtered) {
                                warn!(error = %e, "recording failed, disabling");
                                self.recorder = None;
                            }

                            // Two-pass approach:
                            // 1. Write filtered data to stdout immediately (preserves
                            //    original BSU/ESU timing for normal sync blocks)
                            // 2. Feed sync detector to identify full-redraw blocks
                            //    and strip clear-screen sequences retroactively
                            //
                            // For full-redraw sync blocks, we can't prevent the
                            // initial write, so we use an alternative approach:
                            // detect the full-redraw pattern and overwrite the
                            // screen position after the block completes.

                            // Feed sync detector first to check for full redraws.
                            // Clone filtered to avoid borrow issues — the detector
                            // needs to process the same data we write to stdout.
                            let filtered_owned = filtered.to_vec();
                            let events = detector.process(&filtered_owned);

                            // Check if any sync block needs scroll-jump protection.
                            // A block needs protection if:
                            // 1. It's detected as a full-redraw (ESC[2J + cursor-home), OR
                            // 2. It contains ESC[2J (clear screen), OR
                            // 3. It's large (>10KB) — these are full-screen repaints
                            //    even without explicit clear (e.g., session resume)
                            let needs_protection = events.iter().any(|e| {
                                if let SyncEvent::SyncBlock { data: block_data, is_full_redraw } = e {
                                    *is_full_redraw
                                        || memchr::memmem::find(block_data, b"\x1b[2J").is_some()
                                        || block_data.len() > 10_000
                                } else {
                                    false
                                }
                            });

                            if needs_protection {
                                // Write events individually, stripping scroll-jump
                                // sequences from sync blocks
                                for event in &events {
                                    match event {
                                        SyncEvent::PassThrough(bytes) => {
                                            if let Err(e) = raw_write_all(stdout_handle, bytes) {
                                                error!(error = %e, "failed to write to stdout");
                                                break;
                                            }
                                        }
                                        SyncEvent::SyncBlock { data: block_data, is_full_redraw } => {
                                            let is_large = block_data.len() > 10_000;
                                            let needs_strip = *is_full_redraw
                                                || memchr::memmem::find(block_data, b"\x1b[2J").is_some()
                                                || is_large;
                                            let output_data = if needs_strip {
                                                debug!(
                                                    block_size = block_data.len(),
                                                    is_full_redraw,
                                                    is_large,
                                                    "stripping clear-screen from sync block"
                                                );
                                                strip_clear_screen(block_data)
                                            } else {
                                                block_data.clone()
                                            };

                                            if *is_full_redraw || is_large {
                                                // Large/full-redraw blocks: write WITHOUT
                                                // BSU/ESU. BSU/ESU causes the terminal to
                                                // snap viewport to the active area on ESU,
                                                // pulling the user away from scrollback.
                                                if let Err(e) = raw_write_all(stdout_handle, &output_data) {
                                                    error!(error = %e, "failed to write sync block");
                                                    break;
                                                }
                                            } else {
                                                // Small non-full-redraw: re-wrap in BSU/ESU
                                                let _ = raw_write_all(stdout_handle, b"\x1b[?2026h");
                                                if let Err(e) = raw_write_all(stdout_handle, &output_data) {
                                                    error!(error = %e, "failed to write sync block");
                                                    break;
                                                }
                                                let _ = raw_write_all(stdout_handle, b"\x1b[?2026l");
                                            }
                                        }
                                    }
                                }
                            } else {
                                // No protection needed — write original filtered data
                                // directly, preserving original BSU/ESU timing from ConPTY.
                                if let Err(e) = raw_write_all(stdout_handle, &filtered_owned) {
                                    error!(error = %e, "failed to write to stdout");
                                    break;
                                }
                            }

                            // Feed history from detector events
                            for event in &events {
                                match event {
                                    SyncEvent::PassThrough(bytes) => {
                                        history.push(bytes, HistoryEventType::Output);
                                    }
                                    SyncEvent::SyncBlock { data: block_data, is_full_redraw } => {
                                        if *is_full_redraw {
                                            history.insert_boundary(HistoryEventType::FullRedrawBoundary);
                                        }
                                        history.push(block_data, HistoryEventType::SyncBlock);

                                        let _ = self.event_tx.try_send(
                                            ProxyEvent::SyncBlockComplete {
                                                size_bytes: block_data.len(),
                                                is_full_redraw: *is_full_redraw,
                                            }
                                        );
                                    }
                                }
                            }

                            total_bytes += filtered_owned.len() as u64;
                            chunk_count += 1;
                        }
                        Err(_) => {
                            info!("output channel closed");
                            break;
                        }
                    }
                }
                recv(shutdown_rx) -> msg => {
                    match msg {
                        Ok(reason) => {
                            info!(?reason, "shutdown signal received");
                            break;
                        }
                        Err(_) => {
                            info!("shutdown channel closed");
                            break;
                        }
                    }
                }
                recv(resize_rx) -> msg => {
                    if let Ok((new_cols, new_rows)) = msg
                        && (new_cols, new_rows) != last_size
                    {
                        info!(
                            old_cols = last_size.0,
                            old_rows = last_size.1,
                            new_cols,
                            new_rows,
                            "terminal resize detected"
                        );
                        if let Err(e) = self.session.resize(new_cols, new_rows) {
                            warn!(error = %e, "failed to resize ConPTY");
                        }
                        last_size = (new_cols, new_rows);

                        let _ = self.event_tx.try_send(ProxyEvent::Resize {
                            cols: new_cols,
                            rows: new_rows,
                        });
                    }
                }
            }
        }

        // Disable Kitty keyboard protocol before restoring terminal state
        if let Err(e) = raw_write_all(stdout_handle, KITTY_DISABLE) {
            warn!(error = %e, "failed to send Kitty protocol disable");
        } else {
            info!("Kitty keyboard protocol disabled");
        }

        // Finalize recording if active
        #[cfg(feature = "recording")]
        if let Some(rec) = self.recorder.take()
            && let Err(e) = rec.finish() {
            warn!(error = %e, "failed to finalize vtcap recording");
        }

        // Signal all threads to stop
        info!("shutting down I/O threads");
        shutdown_flag.store(true, Ordering::Relaxed);

        // Signal the input thread's shutdown event so it wakes up from WaitForMultipleObjects
        signal_shutdown_event(shutdown_event_handle);

        // Get exit code before closing ConPTY.
        // The child may have already exited (ChildExited signal) or the output pipe
        // may have closed first (for short-lived commands). Either way, try to get
        // the exit code with a short timeout.
        let exit_code = match self.session.try_wait_for_child(2000) {
            Ok(Some(code)) => {
                info!(exit_code = code, "child exited");
                let _ = self.event_tx.try_send(ProxyEvent::ChildExited {
                    exit_code: code,
                });
                code
            }
            Ok(None) => {
                warn!("child did not exit within timeout");
                0
            }
            Err(e) => {
                warn!(error = %e, "failed to get child exit code");
                0
            }
        };

        // Drop session — this calls ClosePseudoConsole, which closes the ConPTY
        // output pipe. The output thread is still reading from the pipe (its read
        // end is open), so ClosePseudoConsole can flush without deadlocking.
        // After this, the output thread will get pipe EOF and exit.
        info!("closing ConPTY session");
        drop(self.session);

        // Wait for the output thread — it should exit quickly after pipe EOF
        info!("waiting for output thread");
        let _ = output_thread.join();

        // The input thread is NOT joined — on Windows, ReadConsoleInputW on a
        // console handle can block even after WaitForMultipleObjects signals.
        // The thread will be killed when the process exits.
        drop(input_thread);

        info!(
            total_bytes,
            chunk_count,
            "proxy shutdown complete"
        );

        Ok(exit_code)
    }
}

/// Check if stdin is a real console (vs a pipe in test environments).
fn is_stdin_console() -> bool {
    use windows::Win32::System::Console::{GetConsoleMode, GetStdHandle, STD_INPUT_HANDLE};

    unsafe {
        if let Ok(handle) = GetStdHandle(STD_INPUT_HANDLE) {
            let mut mode = windows::Win32::System::Console::CONSOLE_MODE(0);
            GetConsoleMode(handle, &mut mode).is_ok()
        } else {
            false
        }
    }
}

/// Create a manual-reset Windows Event for signaling shutdown to the input thread.
/// Returns the raw handle value as usize (safe to send across threads).
fn create_shutdown_event() -> Result<usize> {
    use windows::Win32::System::Threading::CreateEventW;

    let handle = unsafe {
        CreateEventW(
            None,  // default security
            true,  // manual reset
            false, // initially non-signaled
            None,  // unnamed
        )
        .context("CreateEventW failed")?
    };
    Ok(handle.0 as usize)
}

/// Signal the shutdown event to wake up the input thread.
fn signal_shutdown_event(event_handle: usize) {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::Threading::SetEvent;

    let handle = HANDLE(event_handle as *mut _);
    unsafe {
        let _ = SetEvent(handle);
    }
}

/// Console input loop: uses ReadConsoleInputW to handle both keyboard input
/// and WINDOW_BUFFER_SIZE_EVENT for instant resize detection.
///
/// WaitForMultipleObjects on stdin + shutdown event provides clean cancellation.
/// With ENABLE_VIRTUAL_TERMINAL_INPUT active, the console converts arrow keys,
/// function keys, etc. into VT escape sequences delivered as KEY_EVENT records
/// with valid UnicodeChar values — no manual VT sequence generation needed.
fn run_console_input_loop(
    input_write: crate::conpty::OwnedHandle,
    flag: Arc<AtomicBool>,
    shutdown_tx: Sender<ShutdownReason>,
    shutdown_event_handle: usize,
    resize_tx: Sender<(i16, i16)>,
    tool: ToolKind,
) {
    use windows::Win32::Foundation::{HANDLE, WAIT_OBJECT_0};
    use windows::Win32::System::Console::{
        GetStdHandle, ReadConsoleInputW, INPUT_RECORD, KEY_EVENT, STD_INPUT_HANDLE,
        WINDOW_BUFFER_SIZE_EVENT,
    };
    use windows::Win32::System::Threading::WaitForMultipleObjects;

    let stdin_handle = unsafe { GetStdHandle(STD_INPUT_HANDLE).unwrap_or_default() };
    let event_handle = HANDLE(shutdown_event_handle as *mut _);
    let handles = [stdin_handle, event_handle];
    let mut records = vec![INPUT_RECORD::default(); 128];
    let mut translator = KeyTranslator::new(tool);

    loop {
        if flag.load(Ordering::Relaxed) {
            break;
        }

        // Wait for either stdin events or shutdown event (100ms timeout for flag checks)
        let wait_result = unsafe { WaitForMultipleObjects(&handles, false, 100) };

        if wait_result == WAIT_OBJECT_0 {
            // stdin signaled — read console input records
            let mut num_read = 0u32;
            let read_ok = unsafe {
                ReadConsoleInputW(stdin_handle, &mut records, &mut num_read).is_ok()
            };

            if !read_ok || num_read == 0 {
                info!("stdin EOF");
                let _ = shutdown_tx.try_send(ShutdownReason::InputEof);
                break;
            }

            // Process each input record
            let mut input_bytes = Vec::new();
            for record in &records[..num_read as usize] {
                match record.EventType as u32 {
                    KEY_EVENT => {
                        let key = unsafe { record.Event.KeyEvent };
                        // Only process key-down events
                        if key.bKeyDown.as_bool() {
                            let uc = unsafe { key.uChar.UnicodeChar };
                            if uc != 0 {
                                // Encode UTF-16 code unit to UTF-8
                                if let Some(ch) = char::from_u32(uc as u32) {
                                    let mut buf = [0u8; 4];
                                    let encoded = ch.encode_utf8(&mut buf);
                                    // Handle repeat count
                                    for _ in 0..key.wRepeatCount.max(1) {
                                        input_bytes.extend_from_slice(encoded.as_bytes());
                                    }
                                }
                            }
                        }
                    }
                    WINDOW_BUFFER_SIZE_EVENT => {
                        let size = unsafe { record.Event.WindowBufferSizeEvent };
                        let new_cols = size.dwSize.X;
                        let new_rows = size.dwSize.Y;
                        debug!(cols = new_cols, rows = new_rows, "resize event received");
                        // Use try_send to naturally coalesce rapid resize events
                        let _ = resize_tx.try_send((new_cols, new_rows));
                    }
                    _ => {
                        trace!(event_type = record.EventType, "skipping non-keyboard input event");
                    }
                }
            }

            // Translate Kitty sequences and write to ConPTY
            if !input_bytes.is_empty() {
                let translated = translator.translate(&input_bytes);
                debug!(raw_bytes = input_bytes.len(), translated_bytes = translated.len(), "stdin read");
                if let Err(e) = input_write.write_all(&translated) {
                    if !flag.load(Ordering::Relaxed) {
                        warn!(error = %e, "input pipe write error");
                        let _ = shutdown_tx.try_send(ShutdownReason::IoError(e.to_string()));
                    }
                    break;
                }
            }
        } else if wait_result.0 == WAIT_OBJECT_0.0 + 1 {
            // Shutdown event signaled
            info!("input thread: shutdown event received");
            break;
        }
        // Timeout or error — loop back and check flag
    }

    // Clean up the event handle
    unsafe {
        let _ = windows::Win32::Foundation::CloseHandle(event_handle);
    }
}

/// Pipe input loop: simple blocking read, used when stdin is a pipe (e.g. in tests).
/// The thread will unblock when the pipe is closed or the process exits.
fn run_pipe_input_loop(
    input_write: crate::conpty::OwnedHandle,
    flag: Arc<AtomicBool>,
    shutdown_tx: Sender<ShutdownReason>,
) {
    use std::io::Read;

    let stdin = std::io::stdin();
    let mut stdin = stdin.lock();
    let mut buf = vec![0u8; 1024];

    loop {
        if flag.load(Ordering::Relaxed) {
            break;
        }
        match stdin.read(&mut buf) {
            Ok(0) => {
                info!("stdin EOF");
                let _ = shutdown_tx.try_send(ShutdownReason::InputEof);
                break;
            }
            Ok(n) => {
                debug!(bytes = n, "stdin read");
                if let Err(e) = input_write.write_all(&buf[..n]) {
                    if !flag.load(Ordering::Relaxed) {
                        warn!(error = %e, "input pipe write error");
                        let _ = shutdown_tx.try_send(ShutdownReason::IoError(e.to_string()));
                    }
                    break;
                }
            }
            Err(e) => {
                if !flag.load(Ordering::Relaxed) {
                    warn!(error = %e, "stdin read error");
                    let _ = shutdown_tx.try_send(ShutdownReason::IoError(e.to_string()));
                }
                break;
            }
        }
    }
}

/// Strip CSI 2J (erase display) and cursor-home sequences from a sync block.
/// These sequences cause the terminal to reset scroll position. By removing them
/// from full-redraw sync blocks, the content update happens without scroll jumping.
///
/// Handles all cursor-home variants: ESC[H, ESC[;H, ESC[1;1H, ESC[1H
pub fn strip_clear_screen(data: &[u8]) -> Vec<u8> {
    use memchr::memmem;

    let clear_screen = b"\x1b[2J";

    let mut result = data.to_vec();

    // Strip all occurrences of CSI 2J
    while let Some(pos) = memmem::find(&result, clear_screen) {
        result.drain(pos..pos + clear_screen.len());
    }

    // Strip cursor-home variants only at position 0 — they're often paired with
    // clear screen. Don't strip cursor-home elsewhere as it may be part of
    // legitimate content positioning.
    for pattern in &[
        &b"\x1b[1;1H"[..],
        &b"\x1b[;H"[..],
        &b"\x1b[1H"[..],
        &b"\x1b[H"[..],
    ] {
        if result.starts_with(pattern) {
            result.drain(..pattern.len());
            break;
        }
    }

    result
}

#[cfg(test)]
mod strip_tests {
    use super::*;

    #[test]
    fn test_strip_clear_screen_and_cursor_home() {
        let input = b"\x1b[2J\x1b[Hscreen content here";
        let result = strip_clear_screen(input);
        assert_eq!(result, b"screen content here");
    }

    #[test]
    fn test_strip_clear_screen_only() {
        let input = b"\x1b[2Jcontent";
        let result = strip_clear_screen(input);
        assert_eq!(result, b"content");
    }

    #[test]
    fn test_no_clear_screen_unchanged() {
        let input = b"\x1b[31mred text\x1b[0m";
        let result = strip_clear_screen(input);
        assert_eq!(result, input.to_vec());
    }

    #[test]
    fn test_cursor_home_mid_content_preserved() {
        // CSI H in the middle of content should be preserved
        let input = b"before\x1b[Hafter";
        let result = strip_clear_screen(input);
        assert_eq!(result, b"before\x1b[Hafter");
    }

    #[test]
    fn test_multiple_clear_screens_stripped() {
        let input = b"\x1b[2Jfirst\x1b[2Jsecond";
        let result = strip_clear_screen(input);
        assert_eq!(result, b"firstsecond");
    }

    #[test]
    fn test_empty_input() {
        let result = strip_clear_screen(b"");
        assert!(result.is_empty());
    }

    #[test]
    fn test_strip_cursor_home_variant_1_1() {
        let input = b"\x1b[2J\x1b[1;1Hscreen content";
        let result = strip_clear_screen(input);
        assert_eq!(result, b"screen content");
    }

    #[test]
    fn test_strip_cursor_home_variant_semicolon() {
        let input = b"\x1b[2J\x1b[;Hscreen content";
        let result = strip_clear_screen(input);
        assert_eq!(result, b"screen content");
    }

    #[test]
    fn test_strip_cursor_home_variant_1() {
        let input = b"\x1b[2J\x1b[1Hscreen content";
        let result = strip_clear_screen(input);
        assert_eq!(result, b"screen content");
    }
}

/// Get the raw stdout handle for direct WriteFile access.
/// We bypass Rust's `std::io::stdout()` because it uses `WriteConsoleW` in console mode,
/// which rejects non-UTF-8 byte sequences (e.g., emoji split across ConPTY chunks).
fn raw_stdout_handle() -> windows::Win32::Foundation::HANDLE {
    use windows::Win32::System::Console::{GetStdHandle, STD_OUTPUT_HANDLE};
    unsafe { GetStdHandle(STD_OUTPUT_HANDLE).expect("failed to get stdout handle") }
}

/// Write all bytes to a raw handle using WriteFile.
fn raw_write_all(handle: windows::Win32::Foundation::HANDLE, mut data: &[u8]) -> anyhow::Result<()> {
    use windows::Win32::Storage::FileSystem::WriteFile;
    while !data.is_empty() {
        let mut written = 0u32;
        unsafe {
            WriteFile(handle, Some(data), Some(&mut written), None)
                .map_err(|e| anyhow::anyhow!("WriteFile failed: {e}"))?;
        }
        data = &data[written as usize..];
    }
    Ok(())
}
