// Output sink abstraction — decouples the proxy from stdout.
//
// Phase 1 (CLI proxy) uses StdoutSink which writes directly to the Windows
// console handle with Kitty keyboard protocol management.
// Phase 2 (Tauri GUI) will use TauriIpcSink which emits events to the frontend.

use anyhow::Result;
use std::sync::{Arc, Mutex};
use tracing::{info, warn};

/// Trait for receiving proxy output. Implementations control where
/// processed terminal data goes — stdout, IPC channel, test buffer, etc.
pub trait OutputSink: Send {
    /// Write terminal output data.
    fn write(&self, data: &[u8]) -> Result<()>;

    /// Called once when the proxy starts. StdoutSink uses this to enable
    /// Kitty keyboard protocol; GUI sinks can no-op.
    fn on_startup(&self) {}

    /// Called once when the proxy shuts down. StdoutSink uses this to disable
    /// Kitty keyboard protocol; GUI sinks can no-op.
    fn on_shutdown(&self) {}
}

/// Writes directly to the Windows stdout handle via WriteFile.
/// Manages Kitty keyboard protocol enable/disable on startup/shutdown.
pub struct StdoutSink {
    // Store as usize to satisfy Send — HANDLE(*mut c_void) is not Send.
    // Safe because stdout handle is process-global and valid for the process lifetime.
    handle_raw: usize,
}

impl StdoutSink {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let handle = raw_stdout_handle();
        Self {
            handle_raw: handle.0 as usize,
        }
    }

    fn handle(&self) -> windows::Win32::Foundation::HANDLE {
        windows::Win32::Foundation::HANDLE(self.handle_raw as *mut _)
    }
}

impl OutputSink for StdoutSink {
    fn write(&self, data: &[u8]) -> Result<()> {
        raw_write_all(self.handle(), data)
    }

    fn on_startup(&self) {
        use super::key_translator::KITTY_ENABLE;
        if let Err(e) = raw_write_all(self.handle(), KITTY_ENABLE) {
            warn!(error = %e, "failed to send Kitty protocol enable");
        } else {
            info!("Kitty keyboard protocol enable sent");
        }
    }

    fn on_shutdown(&self) {
        use super::key_translator::KITTY_DISABLE;
        if let Err(e) = raw_write_all(self.handle(), KITTY_DISABLE) {
            warn!(error = %e, "failed to send Kitty protocol disable");
        } else {
            info!("Kitty keyboard protocol disabled");
        }
    }
}

/// Captures output into a shared buffer. Useful for testing.
pub struct BufferSink {
    buffer: Arc<Mutex<Vec<u8>>>,
}

impl BufferSink {
    pub fn new() -> (Self, Arc<Mutex<Vec<u8>>>) {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        (Self { buffer: buffer.clone() }, buffer)
    }
}

impl OutputSink for BufferSink {
    fn write(&self, data: &[u8]) -> Result<()> {
        self.buffer.lock().unwrap().extend_from_slice(data);
        Ok(())
    }
}

/// Get the raw stdout handle for direct WriteFile access.
fn raw_stdout_handle() -> windows::Win32::Foundation::HANDLE {
    use windows::Win32::System::Console::{GetStdHandle, STD_OUTPUT_HANDLE};
    unsafe { GetStdHandle(STD_OUTPUT_HANDLE).expect("failed to get stdout handle") }
}

/// Write all bytes to a raw handle using WriteFile.
pub(crate) fn raw_write_all(handle: windows::Win32::Foundation::HANDLE, mut data: &[u8]) -> anyhow::Result<()> {
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
