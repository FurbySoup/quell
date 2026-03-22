import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Terminal, IDisposable } from "@xterm/xterm";
import { setPasteCallback, clearPasteCallback } from "./terminal";

/// Global event listeners (initialized once, route by session_id)
let listenersInitialized = false;

type OutputHandler = (data: Uint8Array) => void;
type ExitHandler = (exitCode: number) => void;

const outputHandlers = new Map<string, OutputHandler>();
const exitHandlers = new Map<string, ExitHandler>();

/// Per-terminal disposables so we can clean up on reconnect/close
const terminalDisposables = new Map<Terminal, IDisposable[]>();

/// Initialize global event listeners (call once at startup)
export async function initIpcListeners(): Promise<void> {
  if (listenersInitialized) return;
  listenersInitialized = true;

  await listen<{ session_id: string; data: string }>(
    "terminal-output",
    (event) => {
      const handler = outputHandlers.get(event.payload.session_id);
      if (handler) {
        const bytes = Uint8Array.from(atob(event.payload.data), (c) =>
          c.charCodeAt(0),
        );
        handler(bytes);
      }
    },
  );

  await listen<{ session_id: string; exit_code: number }>(
    "child-exited",
    (event) => {
      const handler = exitHandlers.get(event.payload.session_id);
      if (handler) {
        handler(event.payload.exit_code);
      }
    },
  );
}

/// Connect a terminal to a session's IPC events.
/// Disposes any previous handlers on this terminal before registering new ones.
export function connectSession(
  sessionId: string,
  terminal: Terminal,
  defaultCommand?: string,
): void {
  // Dispose previous handlers on this terminal (handles restart case)
  disposeTerminalHandlers(terminal);

  // Route output to this terminal
  outputHandlers.set(sessionId, (data) => {
    terminal.write(data);
  });

  // Handle child exit — offer restart
  exitHandlers.set(sessionId, (exitCode) => {
    terminal.writeln(
      `\r\n\x1b[90m[process exited with code ${exitCode}]\x1b[0m`,
    );
    terminal.writeln(`\r\n\x1b[90mPress any key to restart...\x1b[0m`);

    const disposable = terminal.onData(async () => {
      disposable.dispose();
      terminal.reset();
      try {
        const newId = await spawnSession(
          defaultCommand,
          terminal.cols,
          terminal.rows,
          sessionId,
        );
        // Re-register handlers for the re-spawned session
        connectSession(newId, terminal, defaultCommand);
      } catch (e) {
        terminal.writeln(`\r\n\x1b[31mFailed to restart: ${e}\x1b[0m`);
      }
    });
  });

  // Forward keyboard input to backend
  const dataDisposable = terminal.onData((data) => {
    const encoded = btoa(data);
    invoke("write_input", { sessionId, data: encoded });
  });

  // Forward resize events to backend
  const resizeDisposable = terminal.onResize(({ cols, rows }) => {
    invoke("resize_pty", { sessionId, cols, rows });
  });

  // Register paste callback with the correct session ID
  setPasteCallback(terminal, (text: string) => {
    const encoded = btoa(text);
    invoke("write_input", { sessionId, data: encoded });
  });

  // Track disposables for cleanup
  terminalDisposables.set(terminal, [dataDisposable, resizeDisposable]);
}

/// Dispose all IPC handlers for a terminal (for reconnect or close)
function disposeTerminalHandlers(terminal: Terminal): void {
  const disposables = terminalDisposables.get(terminal);
  if (disposables) {
    for (const d of disposables) {
      d.dispose();
    }
    terminalDisposables.delete(terminal);
  }
  clearPasteCallback(terminal);
}

/// Remove session handlers (for tab close)
export function disconnectSession(
  sessionId: string,
  terminal?: Terminal,
): void {
  outputHandlers.delete(sessionId);
  exitHandlers.delete(sessionId);
  if (terminal) {
    disposeTerminalHandlers(terminal);
  }
}

/// Spawn a terminal session. Returns the session_id.
export async function spawnSession(
  command?: string,
  cols?: number,
  rows?: number,
  sessionId?: string,
): Promise<string> {
  return await invoke<string>("spawn_shell", {
    command,
    cols,
    rows,
    sessionId,
  });
}

/// Close a session by session_id.
export async function closeSession(sessionId: string): Promise<void> {
  await invoke("close_session", { sessionId });
}
