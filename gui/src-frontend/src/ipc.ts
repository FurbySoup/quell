import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Terminal } from "@xterm/xterm";

export async function connectIpc(terminal: Terminal): Promise<void> {
  await listen<{ data: string }>("terminal:output", (event) => {
    const bytes = Uint8Array.from(atob(event.payload.data), (c) => c.charCodeAt(0));
    terminal.write(bytes);
  });

  await listen<{ exitCode: number }>("terminal:exit", (event) => {
    terminal.writeln(`\r\n\x1b[90m[process exited with code ${event.payload.exitCode}]\x1b[0m`);
  });

  await listen<{ message: string }>("terminal:error", (event) => {
    terminal.writeln(`\r\n\x1b[31m[error: ${event.payload.message}]\x1b[0m`);
  });

  terminal.onData((data) => {
    const encoded = btoa(data);
    invoke("terminal_input", { data: encoded });
  });

  terminal.onResize(({ cols, rows }) => {
    invoke("terminal_resize", { cols, rows });
  });
}

export async function spawnSession(command: string, args: string[] = []): Promise<void> {
  await invoke("terminal_spawn", { command, args });
}
