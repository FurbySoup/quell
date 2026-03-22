import { Channel, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Terminal, IDisposable } from "@xterm/xterm";
import { setPasteCallback, clearPasteCallback } from "./terminal";

/// Global exit event listener (initialized once, routes by session_id).
/// Output uses per-session Channels instead of broadcast events.
let exitListenerInitialized = false;

type ExitHandler = (exitCode: number) => void;

const exitHandlers = new Map<string, ExitHandler>();

/// Per-terminal disposables so we can clean up on reconnect/close
const terminalDisposables = new Map<Terminal, IDisposable[]>();

/// Initialize the global child-exited event listener (call once at startup)
export async function initIpcListeners(): Promise<void> {
  if (exitListenerInitialized) return;
  exitListenerInitialized = true;

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
/// Output is handled by the per-session Channel created in spawnSession().
export function connectSession(
  sessionId: string,
  terminal: Terminal,
  cwd?: string,
  args?: string,
  onExit?: () => void,
): void {
  // Dispose previous handlers on this terminal (handles restart case)
  disposeTerminalHandlers(terminal);

  // Handle child exit — offer restart in same directory
  exitHandlers.set(sessionId, (exitCode) => {
    onExit?.();
    terminal.writeln(
      `\r\n\x1b[90m[process exited with code ${exitCode}]\x1b[0m`,
    );
    terminal.writeln(`\r\n\x1b[90mPress any key to restart...\x1b[0m`);

    const offerRestart = () => {
      const disposable = terminal.onData(async () => {
        disposable.dispose();
        terminal.reset();
        try {
          await spawnSession(
            terminal,
            terminal.cols,
            terminal.rows,
            sessionId,
            cwd,
            args,
          );
          connectSession(sessionId, terminal, cwd, args, onExit);
        } catch (e) {
          terminal.writeln(`\r\n\x1b[31m${friendlySpawnError(e)}\x1b[0m`);
          terminal.writeln(
            `\r\n\x1b[90mPress any key to try again...\x1b[0m`,
          );
          offerRestart();
        }
      });
    };
    offerRestart();
  });

  // Forward keyboard input to backend (UTF-8-safe encoding)
  const dataDisposable = terminal.onData((data) => {
    const encoded = btoa(unescape(encodeURIComponent(data)));
    invoke("write_input", { sessionId, data: encoded }).catch((err) => {
      if (!(typeof err === "string" && err.includes("no such session"))) {
        console.warn(`[${sessionId}] write_input failed:`, err);
      }
    });
  });

  // Forward resize events to backend
  const resizeDisposable = terminal.onResize(({ cols, rows }) => {
    invoke("resize_pty", { sessionId, cols, rows }).catch((err) => {
      if (!(typeof err === "string" && err.includes("no such session"))) {
        console.warn(`[${sessionId}] resize_pty failed:`, err);
      }
    });
  });

  // Register paste callback with the correct session ID
  setPasteCallback(terminal, (text: string) => {
    const encoded = btoa(unescape(encodeURIComponent(text)));
    invoke("write_input", { sessionId, data: encoded }).catch((err) => {
      console.warn(`[${sessionId}] paste failed:`, err);
    });
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
  exitHandlers.delete(sessionId);
  if (terminal) {
    disposeTerminalHandlers(terminal);
  }
}

/// Spawn a terminal session with a per-session output Channel.
/// The Channel delivers raw bytes directly as ArrayBuffer — no base64 encoding.
/// The backend reads the command from config — frontend cannot specify it.
export async function spawnSession(
  terminal: Terminal,
  cols?: number,
  rows?: number,
  sessionId?: string,
  cwd?: string,
  args?: string,
): Promise<string> {
  const onOutput = new Channel<ArrayBuffer>();
  onOutput.onmessage = (data) => {
    terminal.write(new Uint8Array(data));
  };

  return await invoke<string>("spawn_shell", {
    onOutput,
    cols,
    rows,
    sessionId,
    cwd,
    args,
  });
}

/// Close a session by session_id.
export async function closeSession(sessionId: string): Promise<void> {
  await invoke("close_session", { sessionId });
}

/// Map raw backend error strings to user-friendly messages.
function friendlySpawnError(raw: unknown): string {
  const msg = String(raw);
  if (msg.includes("failed to spawn")) {
    const match = msg.match(/failed to spawn:\s*(.+)/);
    return match ? `Could not start process: ${match[1]}` : "Could not start process.";
  }
  if (msg.includes("no such session")) return "Session no longer exists.";
  if (msg.includes("input channel closed")) return "Session connection lost.";
  return `Restart failed: ${msg}`;
}
