import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Terminal } from "@xterm/xterm";

export async function connectIpc(terminal: Terminal): Promise<void> {
  // Listen for terminal output from Rust backend (base64 encoded)
  await listen<{ data: string }>("terminal-output", (event) => {
    const bytes = Uint8Array.from(atob(event.payload.data), (c) => c.charCodeAt(0));
    terminal.write(bytes);
  });

  // Listen for child process exit
  await listen<{ exit_code: number }>("child-exited", (event) => {
    terminal.writeln(`\r\n\x1b[90m[process exited with code ${event.payload.exit_code}]\x1b[0m`);
  });

  // Forward keyboard input to Rust backend (base64 encoded)
  terminal.onData((data) => {
    const encoded = btoa(data);
    invoke("write_input", { data: encoded });
  });

  // Forward resize events
  terminal.onResize(({ cols, rows }) => {
    invoke("resize_pty", { cols, rows });
  });
}

// Spawn a terminal session
export async function spawnSession(
  command?: string,
  cols?: number,
  rows?: number,
): Promise<void> {
  await invoke("spawn_shell", { command, cols, rows });
}
