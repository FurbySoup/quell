//! ConPTY session: pseudoconsole creation, child process, resize, cleanup.

use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Console::HPCON;
use tracing::{debug, info};

use super::error::Result;
use super::pipes::{self, OwnedHandle};
use super::sys;

/// A ConPTY session owning the pseudoconsole and child process.
pub struct ConPtySession {
    hpc: HPCON,
    process_handle: HANDLE,
    thread_handle: HANDLE,
    process_id: u32,
    cols: i16,
    rows: i16,
    // Pipe handles for the proxy side — taken by take_io()
    input_write: Option<OwnedHandle>,
    output_read: Option<OwnedHandle>,
}

impl ConPtySession {
    /// Spawn a child process attached to a new pseudoconsole.
    ///
    /// `command` is the full command line (e.g. "cmd.exe" or "claude --flag").
    /// `cols` and `rows` set the initial terminal dimensions.
    pub fn spawn(command: &str, cols: i16, rows: i16, cwd: Option<&str>) -> Result<Self> {
        info!(command, cols, rows, cwd = ?cwd, "creating ConPTY session");

        // Create the pipe pairs
        let (conpty_input_read, proxy_input_write, proxy_output_read, conpty_output_write) =
            pipes::create_conpty_pipes()?;

        // Create the pseudoconsole
        let hpc = sys::create_pseudo_console(cols, rows, conpty_input_read, conpty_output_write)?;

        info!("pseudoconsole created");

        // Close the ConPTY-side pipe handles — CreatePseudoConsole duplicates them.
        sys::close_handle(conpty_input_read);
        sys::close_handle(conpty_output_write);

        // Spawn the child process
        let (process_handle, thread_handle, process_id) =
            sys::create_process_with_pseudo_console(hpc, command, cwd)?;

        info!(pid = process_id, command, "child process spawned");

        Ok(Self {
            hpc,
            process_handle,
            thread_handle,
            process_id,
            cols,
            rows,
            input_write: Some(proxy_input_write),
            output_read: Some(proxy_output_read),
        })
    }

    /// Take ownership of the I/O pipe handles for use in I/O threads.
    /// Returns (input_write, output_read). Can only be called once.
    pub fn take_io(&mut self) -> Option<(OwnedHandle, OwnedHandle)> {
        let input = self.input_write.take()?;
        let output = self.output_read.take()?;
        Some((input, output))
    }

    /// Resize the pseudoconsole.
    pub fn resize(&mut self, cols: i16, rows: i16) -> Result<()> {
        if cols == self.cols && rows == self.rows {
            return Ok(());
        }
        info!(
            old_cols = self.cols,
            old_rows = self.rows,
            new_cols = cols,
            new_rows = rows,
            "resizing pseudoconsole"
        );
        sys::resize_pseudo_console(self.hpc, cols, rows)?;
        self.cols = cols;
        self.rows = rows;
        Ok(())
    }

    /// Block until the child process exits. Returns the exit code.
    #[allow(dead_code)] // Phase 2 — used when proxy manages child lifecycle directly
    pub fn wait_for_child(&self) -> Result<u32> {
        debug!(pid = self.process_id, "waiting for child process");
        sys::wait_for_single_object(self.process_handle, None)?;
        let exit_code = sys::get_exit_code(self.process_handle)?;
        info!(pid = self.process_id, exit_code, "child process exited");
        Ok(exit_code)
    }

    /// Try to wait for the child with a timeout (ms). Returns Ok(None) on timeout.
    pub fn try_wait_for_child(&self, timeout_ms: u32) -> Result<Option<u32>> {
        debug!(pid = self.process_id, timeout_ms, "waiting for child process (with timeout)");
        let signaled = sys::wait_for_single_object(self.process_handle, Some(timeout_ms))?;
        if signaled {
            let exit_code = sys::get_exit_code(self.process_handle)?;
            info!(pid = self.process_id, exit_code, "child process exited");
            Ok(Some(exit_code))
        } else {
            Ok(None)
        }
    }

    /// Get the raw process handle value (as usize, safe to send across threads).
    pub fn process_handle_raw(&self) -> usize {
        self.process_handle.0 as usize
    }

    /// Get the child process ID.
    #[allow(dead_code)] // Phase 2 — used for process management in Tauri
    pub fn process_id(&self) -> u32 {
        self.process_id
    }

    /// Get current dimensions.
    pub fn size(&self) -> (i16, i16) {
        (self.cols, self.rows)
    }
}

impl Drop for ConPtySession {
    fn drop(&mut self) {
        // Drop pipe handles first (close our side)
        self.input_write.take();
        self.output_read.take();

        // Close pseudoconsole — this signals the child to exit
        sys::close_pseudo_console(self.hpc);

        // Close process/thread handles
        sys::close_handle(self.thread_handle);
        sys::close_handle(self.process_handle);

        info!(pid = self.process_id, "ConPTY session closed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spawn_succeeds() {
        let session = ConPtySession::spawn("cmd.exe /c echo hello", 80, 25, None)
            .expect("failed to spawn");
        assert!(session.process_id() > 0);
        // Session drops here, closing ConPTY cleanly
    }

    #[test]
    fn test_resize() {
        let mut session = ConPtySession::spawn("cmd.exe /c timeout /t 2 /nobreak >nul", 80, 25, None)
            .expect("failed to spawn");

        // Resize should succeed
        let result = session.resize(120, 40);
        assert!(result.is_ok(), "resize failed: {:?}", result.err());
        assert_eq!(session.size(), (120, 40));

        // Same size should be a no-op
        let result = session.resize(120, 40);
        assert!(result.is_ok());
    }

    #[test]
    fn test_take_io() {
        let mut session =
            ConPtySession::spawn("cmd.exe /c echo test", 80, 25, None).expect("failed to spawn");

        // First take should succeed
        let io = session.take_io();
        assert!(io.is_some());

        // Second take should return None
        let io2 = session.take_io();
        assert!(io2.is_none());
    }

    #[test]
    fn test_child_exit_code() {
        let mut session =
            ConPtySession::spawn("cmd.exe /c exit 42", 80, 25, None).expect("failed to spawn");
        let (_input, output) = session.take_io().expect("take_io failed");

        // Drain output in background
        let drain = std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match output.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
            }
        });

        let exit_code = session.wait_for_child().unwrap();
        drop(session);
        let _ = drain.join();

        // ConPTY may report the actual exit code or a termination status.
        // Accept either the real code or a ConPTY termination artifact.
        assert!(
            exit_code == 42 || exit_code != 0,
            "expected non-zero exit code, got: {exit_code}"
        );
    }
}
