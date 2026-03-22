import { invoke } from "@tauri-apps/api/core";
import {
  createTerminal,
  activateTerminal,
  deactivateTerminal,
  getTheme,
  ThemeMode,
  TerminalInstance,
} from "./terminal";
import {
  initIpcListeners,
  connectSession,
  disconnectSession,
  spawnSession,
  closeSession,
} from "./ipc";

interface Session {
  id: string;
  instance: TerminalInstance;
  tabEl: HTMLDivElement;
}

let sessions: Session[] = [];
let activeSessionId: string | null = null;
let themeMode: ThemeMode = "dark";
let defaultCommand: string | undefined;

function getTabsEl(): HTMLElement {
  return document.getElementById("tabs")!;
}

function getTerminalsEl(): HTMLElement {
  return document.getElementById("terminals")!;
}

function createTabElement(sessionId: string, index: number): HTMLDivElement {
  const tab = document.createElement("div");
  tab.className = "tab";
  tab.dataset.sessionId = sessionId;
  tab.innerHTML = `<span class="label">Tab ${index}</span><span class="close">\u00d7</span>`;

  tab.addEventListener("click", (e) => {
    const target = e.target as HTMLElement;
    if (target.classList.contains("close")) {
      removeSession(sessionId);
    } else {
      switchToSession(sessionId);
    }
  });

  return tab;
}

function switchToSession(sessionId: string): void {
  if (activeSessionId === sessionId) return;

  // Deactivate current
  const current = sessions.find((s) => s.id === activeSessionId);
  if (current) {
    deactivateTerminal(current.instance);
    current.tabEl.classList.remove("active");
  }

  // Activate target
  const target = sessions.find((s) => s.id === sessionId);
  if (target) {
    activateTerminal(target.instance);
    target.tabEl.classList.add("active");
    activeSessionId = sessionId;
  }
}

async function addSession(): Promise<void> {
  const terminalsEl = getTerminalsEl();
  const tabsEl = getTabsEl();

  // Create terminal instance
  const tempId = `pending-${Date.now()}`;
  const instance = createTerminal(terminalsEl, tempId, themeMode);

  // Spawn backend session
  const sessionId = await spawnSession(
    defaultCommand,
    instance.terminal.cols,
    instance.terminal.rows,
  );

  // Update the instance's session ID
  instance.sessionId = sessionId;
  instance.container.dataset.sessionId = sessionId;

  // Create tab
  const tabEl = createTabElement(sessionId, sessions.length + 1);
  tabsEl.appendChild(tabEl);

  const session: Session = { id: sessionId, instance, tabEl };
  sessions.push(session);

  // Wire IPC
  connectSession(sessionId, instance.terminal, defaultCommand);

  // Wire resize — the window resize handler needs to fit the active terminal
  // (handled globally below)

  // Switch to this tab
  switchToSession(sessionId);
}

async function removeSession(sessionId: string): Promise<void> {
  const idx = sessions.findIndex((s) => s.id === sessionId);
  if (idx === -1) return;

  const session = sessions[idx];

  // Disconnect IPC handlers
  disconnectSession(sessionId, session.instance.terminal);

  // Close backend session (ignore errors if already exited)
  try {
    await closeSession(sessionId);
  } catch {
    // Session may have already exited
  }

  // Remove DOM elements
  session.tabEl.remove();
  session.instance.terminal.dispose();
  session.instance.container.remove();

  // Remove from list
  sessions.splice(idx, 1);

  // If we closed the active tab, switch to another
  if (activeSessionId === sessionId) {
    activeSessionId = null;
    if (sessions.length > 0) {
      const newIdx = Math.min(idx, sessions.length - 1);
      switchToSession(sessions[newIdx].id);
    }
  }

  // Renumber tabs
  sessions.forEach((s, i) => {
    const label = s.tabEl.querySelector(".label");
    if (label) label.textContent = `Tab ${i + 1}`;
  });
}

document.addEventListener("DOMContentLoaded", async () => {
  // Detect system theme and default command from Rust backend
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

  // Apply theme to page
  const theme = getTheme(themeMode);
  document.body.style.background = theme.background ?? "#1e1e1e";
  if (themeMode === "light") {
    document.body.classList.add("light");
  }

  // Initialize global IPC listeners
  await initIpcListeners();

  // Create first tab
  await addSession();

  // New tab button
  document.getElementById("tab-new")!.addEventListener("click", () => {
    addSession();
  });

  // Global resize handler — fit the active terminal.
  // xterm.js onResize (in ipc.ts) forwards the new dimensions to the backend.
  let resizeTimeout: ReturnType<typeof setTimeout> | null = null;
  window.addEventListener("resize", () => {
    if (resizeTimeout) clearTimeout(resizeTimeout);
    resizeTimeout = setTimeout(() => {
      const active = sessions.find((s) => s.id === activeSessionId);
      if (active) {
        active.instance.fitAddon.fit();
      }
    }, 100);
  });
});
