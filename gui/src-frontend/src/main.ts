import { initTerminal } from "./terminal";

document.addEventListener("DOMContentLoaded", () => {
  const container = document.getElementById("terminal");
  if (!container) return;

  const terminal = initTerminal(container);

  // Temporary: local echo for verification
  terminal.writeln("\x1b[1;36mQuell Terminal\x1b[0m v0.1.0");
  terminal.writeln("Type to test local echo. IPC bridge coming next.\r\n");

  terminal.onData((data) => {
    // Echo typed characters (temporary — will be replaced by IPC bridge)
    terminal.write(data);
  });
});
