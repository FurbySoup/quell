import { invoke } from "@tauri-apps/api/core";
import { initTerminal, getTheme, ThemeMode } from "./terminal";
import { connectIpc, spawnSession } from "./ipc";

document.addEventListener("DOMContentLoaded", async () => {
  const container = document.getElementById("terminal");
  if (!container) return;

  // Detect system theme and default command from Rust backend
  let themeMode: ThemeMode = "dark";
  let defaultCommand: string | undefined;

  try {
    themeMode = (await invoke<string>("get_system_theme")) as ThemeMode;
  } catch (e) {
    console.warn("get_system_theme failed, defaulting to dark:", e);
  }

  try {
    defaultCommand = await invoke<string>("get_default_command");
  } catch (e) {
    console.warn("get_default_command failed:", e);
  }

  // Apply theme to page background
  const theme = getTheme(themeMode);
  document.body.style.background = theme.background ?? "#1e1e1e";

  const terminal = initTerminal(container, undefined, themeMode);

  // Connect IPC bridge then spawn shell with terminal dimensions
  await connectIpc(terminal);
  await spawnSession(defaultCommand, terminal.cols, terminal.rows);
});
