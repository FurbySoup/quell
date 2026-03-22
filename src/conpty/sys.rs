//! Safe wrappers around Windows API calls for ConPTY.
//!
//! All unsafe code is contained in this module. Every function provides
//! a safe Rust interface over the raw Win32 API.

use std::mem;

use windows::Win32::Foundation::{
    CloseHandle, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0,
};
use windows::Win32::Storage::FileSystem::{ReadFile, WriteFile};
use windows::Win32::System::Console::{
    ClosePseudoConsole, CreatePseudoConsole, GetConsoleMode, GetConsoleScreenBufferInfo,
    GetStdHandle, ResizePseudoConsole, SetConsoleMode, CONSOLE_MODE,
    CONSOLE_SCREEN_BUFFER_INFO, COORD, HPCON, STD_HANDLE,
};
use windows::Win32::System::Pipes::CreatePipe;
use windows::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, GetExitCodeProcess,
    InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
    UpdateProcThreadAttribute, WaitForSingleObject, INFINITE,
    PROCESS_INFORMATION, STARTUPINFOEXW,
};

use tracing::{debug, trace};

use super::error::{ConPtyError, Result};

const PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE: usize = 0x00020016;

/// Create an anonymous pipe. Returns (read_handle, write_handle).
/// No inheritance attributes — ConPTY handles duplication internally.
pub(super) fn create_pipe() -> Result<(HANDLE, HANDLE)> {
    trace!("creating anonymous pipe");
    let mut read_handle = HANDLE::default();
    let mut write_handle = HANDLE::default();

    unsafe {
        CreatePipe(&mut read_handle, &mut write_handle, None, 0)
            .map_err(|e| ConPtyError::PipeCreation { source: e })?;
    }

    Ok((read_handle, write_handle))
}

/// Create a pseudoconsole with the given dimensions.
pub(super) fn create_pseudo_console(
    cols: i16,
    rows: i16,
    input_read: HANDLE,
    output_write: HANDLE,
) -> Result<HPCON> {
    debug!(cols, rows, "creating pseudoconsole");
    let size = COORD { X: cols, Y: rows };
    unsafe {
        CreatePseudoConsole(size, input_read, output_write, 0)
            .map_err(|e| ConPtyError::PseudoConsoleCreation {
                cols,
                rows,
                source: e,
            })
    }
}

/// Resize the pseudoconsole.
pub(super) fn resize_pseudo_console(hpc: HPCON, cols: i16, rows: i16) -> Result<()> {
    debug!(cols, rows, "resizing pseudoconsole");
    let size = COORD { X: cols, Y: rows };
    unsafe {
        ResizePseudoConsole(hpc, size).map_err(|e| ConPtyError::Resize {
            cols,
            rows,
            source: e,
        })
    }
}

/// Close the pseudoconsole.
pub(super) fn close_pseudo_console(hpc: HPCON) {
    debug!("closing pseudoconsole");
    unsafe {
        ClosePseudoConsole(hpc);
    }
}

/// Close a raw HANDLE.
pub(super) fn close_handle(handle: HANDLE) {
    if !handle.is_invalid() {
        unsafe {
            let _ = CloseHandle(handle);
        }
    }
}

/// Read from a file/pipe handle into a buffer. Returns bytes read.
/// Returns 0 on EOF or broken pipe.
pub(super) fn read_file(handle: HANDLE, buf: &mut [u8]) -> Result<u32> {
    let mut bytes_read = 0u32;
    unsafe {
        ReadFile(handle, Some(buf), Some(&mut bytes_read), None).map_err(|e| {
            ConPtyError::PipeRead { source: e }
        })?;
    }
    Ok(bytes_read)
}

/// Write to a file/pipe handle. Returns bytes written.
pub(super) fn write_file(handle: HANDLE, buf: &[u8]) -> Result<u32> {
    let mut bytes_written = 0u32;
    unsafe {
        WriteFile(handle, Some(buf), Some(&mut bytes_written), None).map_err(|e| {
            ConPtyError::PipeWrite { source: e }
        })?;
    }
    Ok(bytes_written)
}

/// Get the console mode for a handle.
pub(super) fn get_console_mode(handle: HANDLE) -> Result<CONSOLE_MODE> {
    let mut mode = CONSOLE_MODE::default();
    unsafe {
        GetConsoleMode(handle, &mut mode).map_err(|e| ConPtyError::ConsoleModeGet { source: e })?;
    }
    Ok(mode)
}

/// Set the console mode for a handle.
pub(super) fn set_console_mode(handle: HANDLE, mode: CONSOLE_MODE) -> Result<()> {
    unsafe {
        SetConsoleMode(handle, mode).map_err(|e| ConPtyError::ConsoleModeSet { source: e })
    }
}

/// Get the standard handle (stdin, stdout, stderr).
pub(super) fn get_std_handle(which: STD_HANDLE) -> Result<HANDLE> {
    unsafe {
        GetStdHandle(which).map_err(|e| ConPtyError::ConsoleModeGet { source: e })
    }
}

/// Get the console screen buffer info (for detecting terminal size).
pub(super) fn get_console_screen_buffer_info(
    handle: HANDLE,
) -> Result<CONSOLE_SCREEN_BUFFER_INFO> {
    let mut info = CONSOLE_SCREEN_BUFFER_INFO::default();
    unsafe {
        GetConsoleScreenBufferInfo(handle, &mut info)
            .map_err(|e| ConPtyError::ConsoleModeGet { source: e })?;
    }
    Ok(info)
}

/// Spawn a process attached to a pseudoconsole.
/// Returns (process_handle, thread_handle, process_id).
pub(super) fn create_process_with_pseudo_console(
    hpc: HPCON,
    command_line: &str,
    cwd: Option<&str>,
) -> Result<(HANDLE, HANDLE, u32)> {
    debug!(command_line, cwd = ?cwd, "spawning process with pseudoconsole");
    // Allocate the attribute list — first call gets the required size
    let mut attr_list_size: usize = 0;
    unsafe {
        let _ = InitializeProcThreadAttributeList(
            None,
            1,
            Some(0),
            &mut attr_list_size,
        );
    }

    // Allocate buffer and create the LPPROC_THREAD_ATTRIBUTE_LIST
    let mut attr_list_buf: Vec<u8> = vec![0; attr_list_size];
    let attr_list = LPPROC_THREAD_ATTRIBUTE_LIST(attr_list_buf.as_mut_ptr() as *mut _);

    unsafe {
        InitializeProcThreadAttributeList(
            Some(attr_list),
            1,
            Some(0),
            &mut attr_list_size,
        )
        .map_err(|e| ConPtyError::AttributeList { source: e })?;

        // Set the pseudoconsole attribute.
        // Pass the raw HPCON handle value as the pointer — the C API expects
        // the handle value itself in lpValue, not a pointer to the handle variable.
        let hpc_raw = hpc.0 as *const std::ffi::c_void;
        UpdateProcThreadAttribute(
            attr_list,
            0,
            PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE,
            Some(hpc_raw),
            mem::size_of::<HPCON>(),
            None,
            None,
        )
        .map_err(|e| ConPtyError::AttributeList { source: e })?;
    }

    // Build the command line as a wide string
    let mut cmd_wide: Vec<u16> = command_line.encode_utf16().collect();
    cmd_wide.push(0);

    let mut startup_info: STARTUPINFOEXW = unsafe { mem::zeroed() };
    startup_info.StartupInfo.cb = mem::size_of::<STARTUPINFOEXW>() as u32;
    // STARTF_USESTDHANDLES with INVALID_HANDLE_VALUE prevents the child from
    // inheriting the parent's redirected handles (e.g., when running under an IDE).
    startup_info.StartupInfo.dwFlags =
        windows::Win32::System::Threading::STARTF_USESTDHANDLES;
    startup_info.StartupInfo.hStdInput = INVALID_HANDLE_VALUE;
    startup_info.StartupInfo.hStdOutput = INVALID_HANDLE_VALUE;
    startup_info.StartupInfo.hStdError = INVALID_HANDLE_VALUE;
    startup_info.lpAttributeList = attr_list;

    let mut proc_info = PROCESS_INFORMATION::default();

    // Convert CWD to wide string if provided
    let cwd_wide: Option<Vec<u16>> = cwd.map(|d| {
        let mut w: Vec<u16> = d.encode_utf16().collect();
        w.push(0);
        w
    });
    let cwd_pcwstr = match &cwd_wide {
        Some(w) => windows::core::PCWSTR(w.as_ptr()),
        None => windows::core::PCWSTR::null(),
    };

    let result = unsafe {
        CreateProcessW(
            None,
            Some(windows::core::PWSTR(cmd_wide.as_mut_ptr())),
            None,
            None,
            false,
            windows::Win32::System::Threading::EXTENDED_STARTUPINFO_PRESENT
                | windows::Win32::System::Threading::CREATE_UNICODE_ENVIRONMENT,
            None,
            cwd_pcwstr,
            &startup_info.StartupInfo,
            &mut proc_info,
        )
    };

    // Clean up attribute list regardless of result
    unsafe {
        DeleteProcThreadAttributeList(attr_list);
    }

    result.map_err(|e| ConPtyError::ProcessSpawn {
        command: command_line.to_string(),
        source: e,
    })?;

    Ok((proc_info.hProcess, proc_info.hThread, proc_info.dwProcessId))
}

/// Wait for a process to exit. Returns immediately if already exited.
/// If `timeout_ms` is None, waits indefinitely.
pub(super) fn wait_for_single_object(handle: HANDLE, timeout_ms: Option<u32>) -> Result<bool> {
    let timeout = timeout_ms.unwrap_or(INFINITE);
    let result = unsafe { WaitForSingleObject(handle, timeout) };
    Ok(result == WAIT_OBJECT_0)
}

/// Get the exit code of a process.
pub(super) fn get_exit_code(process_handle: HANDLE) -> Result<u32> {
    let mut exit_code = 0u32;
    unsafe {
        GetExitCodeProcess(process_handle, &mut exit_code)
            .map_err(|e| ConPtyError::WaitFailed { source: e })?;
    }
    Ok(exit_code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipe_creation() {
        let result = create_pipe();
        assert!(result.is_ok());
        let (read_h, write_h) = result.unwrap();
        assert!(!read_h.is_invalid());
        assert!(!write_h.is_invalid());
        close_handle(read_h);
        close_handle(write_h);
    }

    #[test]
    fn test_pipe_roundtrip() {
        let (read_h, write_h) = create_pipe().unwrap();

        let data = b"hello conpty";
        let written = write_file(write_h, data).unwrap();
        assert_eq!(written as usize, data.len());

        let mut buf = [0u8; 64];
        let read = read_file(read_h, &mut buf).unwrap();
        assert_eq!(read as usize, data.len());
        assert_eq!(&buf[..read as usize], data);

        close_handle(read_h);
        close_handle(write_h);
    }

    #[test]
    fn test_pipe_close_detection() {
        let (read_h, write_h) = create_pipe().unwrap();

        // Close write end
        close_handle(write_h);

        // Read should return 0 or error (broken pipe)
        let mut buf = [0u8; 64];
        let result = read_file(read_h, &mut buf);
        // On Windows, reading from a pipe with closed write end returns 0 bytes or ERROR_BROKEN_PIPE
        match result {
            Ok(0) => {} // EOF
            Err(_) => {} // Broken pipe error is also acceptable
            Ok(n) => panic!("expected 0 bytes or error, got {n} bytes"),
        }

        close_handle(read_h);
    }

    #[test]
    fn test_console_mode() {
        use windows::Win32::System::Console::STD_OUTPUT_HANDLE;
        // This test may fail in CI where there's no real console
        let handle = get_std_handle(STD_OUTPUT_HANDLE);
        if let Ok(handle) = handle {
            // Just verify get_console_mode doesn't crash
            let _mode = get_console_mode(handle);
        }
    }
}
