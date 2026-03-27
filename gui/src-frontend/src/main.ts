import {
  createTerminal,
  activateTerminal,
  deactivateTerminal,
  registerShortcuts,
  TerminalInstance,
} from "./terminal";
import {
  initIpcListeners,
  connectSession,
  disconnectSession,
  spawnSession,
  closeSession,
} from "./ipc";
import {
  loadPreferences,
  savePreference,
  DEFAULTS,
} from "./preferences";
import { open as openDialog, confirm } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  getAllThemes,
  getThemeByName,
  applyQuellTheme,
  toXtermTheme,
  QuellTheme,
} from "./themes";
import {
  initSearchUI,
  setActiveSearchAddon,
  setSearchTheme,
  refreshDecorations,
  toggleSearch,
  findNext,
  findPrevious,
} from "./search";
import {
  initPaletteUI,
  registerActions,
  togglePalette,
} from "./palette";

interface Session {
  id: string;
  instance: TerminalInstance;
  tabEl: HTMLDivElement;
  customName: boolean;
  cwd: string;
  args: string;
  exited: boolean;
  streaming: boolean;
  unread: boolean;
  streamingTimer: ReturnType<typeof setTimeout> | null;
}

let sessions: Session[] = [];
let activeSessionId: string | null = null;
let currentTheme: QuellTheme = getThemeByName(DEFAULTS.themePref);

// Zoom state
const MIN_FONT_SIZE = 8;
const MAX_FONT_SIZE = 32;
const FONT_STEP = 2;
let currentFontSize = DEFAULTS.fontSize;
let currentThemePref: string = DEFAULTS.themePref;
let currentArgs: string = DEFAULTS.defaultArgs;
let spawning = false;

// --- Activity indicators ---

const STREAMING_TIMEOUT = 1500;

function handleSessionOutput(sessionId: string): void {
  const session = sessions.find((s) => s.id === sessionId);
  if (!session) return;

  session.streaming = true;
  if (session.streamingTimer) clearTimeout(session.streamingTimer);
  session.streamingTimer = setTimeout(() => {
    session.streaming = false;
    updateActivityIndicators(sessionId);
  }, STREAMING_TIMEOUT);

  if (sessionId !== activeSessionId) {
    session.unread = true;
  }

  updateActivityIndicators(sessionId);
}

function updateActivityIndicators(sessionId: string): void {
  const session = sessions.find((s) => s.id === sessionId);
  if (!session) return;

  session.tabEl.classList.toggle("streaming", session.streaming);
  session.tabEl.classList.toggle("unread", session.unread && !session.streaming);
  session.instance.container.classList.toggle("streaming", session.streaming);
}

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

  const label = document.createElement("span");
  label.className = "label";
  label.textContent = `Tab ${index}`;
  tab.appendChild(label);

  const close = document.createElement("span");
  close.className = "close";
  close.textContent = "\u00d7";
  tab.appendChild(close);

  tab.addEventListener("click", (e) => {
    const target = e.target as HTMLElement;
    if (target.classList.contains("close")) {
      removeSession(sessionId);
    } else {
      switchToSession(sessionId);
    }
  });

  label.addEventListener("dblclick", (e) => {
    e.stopPropagation();
    const labelEl = e.target as HTMLElement;
    const session = sessions.find((s) => s.id === sessionId);
    if (!session) return;

    const input = document.createElement("input");
    input.className = "tab-rename";
    input.value = labelEl.textContent ?? "";
    input.style.width = `${Math.max(labelEl.offsetWidth, 40)}px`;

    const commit = () => {
      const name = input.value.trim();
      if (name) {
        labelEl.textContent = name;
        session.customName = true;
      }
      input.replaceWith(labelEl);
    };

    input.addEventListener("keydown", (ke) => {
      if (ke.key === "Enter") commit();
      if (ke.key === "Escape") input.replaceWith(labelEl);
    });
    input.addEventListener("blur", commit);

    labelEl.replaceWith(input);
    input.select();
    input.focus();
  });

  return tab;
}

function switchToSession(sessionId: string): void {
  if (activeSessionId === sessionId) return;

  const current = sessions.find((s) => s.id === activeSessionId);
  if (current) {
    deactivateTerminal(current.instance);
    current.tabEl.classList.remove("active");
  }

  const target = sessions.find((s) => s.id === sessionId);
  if (target) {
    target.unread = false;
    target.tabEl.classList.remove("unread");
    activateTerminal(target.instance);
    target.tabEl.classList.add("active");
    activeSessionId = sessionId;
    setActiveSearchAddon(target.instance.searchAddon);
  }
}

async function pickFolder(): Promise<string | null> {
  return await openDialog({
    directory: true,
    multiple: false,
    title: "Choose project folder",
    defaultPath: lastCwd,
  });
}

let lastCwd: string | undefined;

function showWelcome(): void {
  const terminalsEl = getTerminalsEl();
  let welcome = document.getElementById("welcome-screen");
  if (welcome) return;

  welcome = document.createElement("div");
  welcome.id = "welcome-screen";

  const content = document.createElement("div");
  content.className = "welcome-content";

  // > quell prompt line
  const prompt = document.createElement("div");
  prompt.className = "welcome-prompt";
  const chevron = document.createElement("span");
  chevron.className = "welcome-chevron";
  chevron.textContent = ">";
  prompt.appendChild(chevron);
  const title = document.createElement("span");
  title.className = "welcome-title";
  title.textContent = "quell";
  prompt.appendChild(title);
  content.appendChild(prompt);

  // Separator
  const rule = document.createElement("div");
  rule.className = "welcome-rule";
  content.appendChild(rule);

  // Info lines
  const line1 = document.createElement("div");
  line1.className = "welcome-line";
  line1.textContent = "terminal for AI CLI tools";
  content.appendChild(line1);

  const line2 = document.createElement("div");
  line2.className = "welcome-line-small";
  line2.textContent = "select a project folder to begin";
  content.appendChild(line2);

  // Command button
  const cmd = document.createElement("button");
  cmd.className = "welcome-cmd";
  const cmdChevron = document.createElement("span");
  cmdChevron.className = "welcome-cmd-chevron";
  cmdChevron.textContent = ">";
  cmd.appendChild(cmdChevron);
  cmd.appendChild(document.createTextNode(" open folder"));
  cmd.addEventListener("click", () => addSession());
  content.appendChild(cmd);

  // Shortcut hints
  const shortcuts = document.createElement("div");
  shortcuts.className = "welcome-shortcuts";

  const hint1 = document.createElement("div");
  hint1.className = "welcome-shortcut";
  hint1.innerHTML = "";
  const kbd1 = document.createElement("kbd");
  kbd1.textContent = "Ctrl+Shift+P";
  hint1.appendChild(kbd1);
  hint1.appendChild(document.createTextNode("  command palette"));
  shortcuts.appendChild(hint1);

  const hint2 = document.createElement("div");
  hint2.className = "welcome-shortcut";
  const kbd2 = document.createElement("kbd");
  kbd2.textContent = "Ctrl+Shift+N";
  hint2.appendChild(kbd2);
  hint2.appendChild(document.createTextNode("  new tab"));
  shortcuts.appendChild(hint2);

  content.appendChild(shortcuts);
  welcome.appendChild(content);
  terminalsEl.appendChild(welcome);
}

function hideWelcome(): void {
  document.getElementById("welcome-screen")?.remove();
}

async function addSessionWithPicker(): Promise<void> {
  const picked = await pickFolder();
  if (!picked) return;
  await addSession(picked);
}

async function addSession(cwd?: string): Promise<void> {
  if (spawning) return;
  spawning = true;
  try {
    // If no cwd: inherit from active session, or prompt picker if no sessions exist
    if (!cwd) {
      const active = sessions.find((s) => s.id === activeSessionId);
      if (active) {
        cwd = active.cwd;
      } else {
        const picked = await pickFolder();
        if (!picked) return;
        cwd = picked;
      }
    }

    const terminalsEl = getTerminalsEl();
    const tabsEl = getTabsEl();

    const sessionId = crypto.randomUUID();
    const xtermTheme = toXtermTheme(currentTheme);
    const instance = createTerminal(
      terminalsEl,
      sessionId,
      xtermTheme,
      currentFontSize,
    );

    lastCwd = cwd;
    hideWelcome();

    const outputCallback = () => handleSessionOutput(sessionId);

    try {
      await spawnSession(
        instance.terminal,
        instance.terminal.cols,
        instance.terminal.rows,
        sessionId,
        cwd,
        currentArgs,
        outputCallback,
      );
    } catch (e) {
      // Clean up orphaned DOM from the failed spawn
      instance.terminal.dispose();
      instance.container.remove();
      if (sessions.length === 0) showWelcome();
      console.error("Failed to spawn session:", e);
      return;
    }

    const tabEl = createTabElement(sessionId, sessions.length + 1);
    tabsEl.appendChild(tabEl);

    const session: Session = {
      id: sessionId,
      instance,
      tabEl,
      customName: false,
      cwd: cwd!,
      args: currentArgs,
      exited: false,
      streaming: false,
      unread: false,
      streamingTimer: null,
    };
    sessions.push(session);

    connectSession(sessionId, instance.terminal, cwd, () => currentArgs, () => {
      session.exited = true;
      session.streaming = false;
      if (session.streamingTimer) clearTimeout(session.streamingTimer);
      updateActivityIndicators(sessionId);
    }, outputCallback);

    switchToSession(sessionId);
    updateTabBarVisibility();
  } finally {
    spawning = false;
  }
}

async function removeSession(sessionId: string): Promise<void> {
  const idx = sessions.findIndex((s) => s.id === sessionId);
  if (idx === -1) return;

  const session = sessions[idx];

  if (!session.exited) {
    const confirmed = await confirm(
      "Close this tab? The running process will be terminated.",
      { title: "Quell" },
    );
    if (!confirmed) return;
  }

  if (session.streamingTimer) clearTimeout(session.streamingTimer);
  disconnectSession(sessionId, session.instance.terminal);

  try {
    await closeSession(sessionId);
  } catch {
    // Session may have already exited
  }

  session.tabEl.remove();
  session.instance.terminal.dispose();
  session.instance.container.remove();

  sessions.splice(idx, 1);

  if (activeSessionId === sessionId) {
    activeSessionId = null;
    if (sessions.length > 0) {
      const newIdx = Math.min(idx, sessions.length - 1);
      switchToSession(sessions[newIdx].id);
    }
  }

  sessions.forEach((s, i) => {
    if (!s.customName) {
      const label = s.tabEl.querySelector(".label");
      if (label) label.textContent = `Tab ${i + 1}`;
    }
  });

  updateTabBarVisibility();
}

function updateTabBarVisibility(): void {
  const tabBar = document.getElementById("tab-bar")!;
  if (sessions.length <= 1) {
    tabBar.classList.add("single-tab");
  } else {
    tabBar.classList.remove("single-tab");
  }
  const active = sessions.find((s) => s.id === activeSessionId);
  if (active) {
    active.instance.fitAddon.fit();
  }
}

// --- Zoom ---

function setFontSize(size: number): void {
  currentFontSize = Math.max(MIN_FONT_SIZE, Math.min(MAX_FONT_SIZE, size));
  for (const s of sessions) {
    s.instance.terminal.options.fontSize = currentFontSize;
  }
  const active = sessions.find((s) => s.id === activeSessionId);
  if (active) {
    active.instance.fitAddon.fit();
  }
  // Scale chrome UI proportionally with terminal font size
  const scale = currentFontSize / DEFAULTS.fontSize;
  document.documentElement.style.setProperty("--q-ui-scale", String(scale));
  savePreference("fontSize", currentFontSize);
}

// --- Theme ---

function applyTheme(themeName: string): void {
  currentTheme = getThemeByName(themeName);
  currentThemePref = themeName;
  applyQuellTheme(currentTheme);
  const xtermTheme = toXtermTheme(currentTheme);
  for (const s of sessions) {
    s.instance.terminal.options.theme = xtermTheme;
  }
  setSearchTheme(currentTheme);
  refreshDecorations();
}

/** Apply theme visuals without updating saved state — used for palette live preview. */
function previewTheme(themeName: string): void {
  const theme = getThemeByName(themeName);
  applyQuellTheme(theme);
  const xtermTheme = toXtermTheme(theme);
  for (const s of sessions) {
    s.instance.terminal.options.theme = xtermTheme;
  }
  setSearchTheme(theme);
  refreshDecorations();
}

function resolveThemeName(pref: string): string {
  if (pref === "system") {
    return window.matchMedia("(prefers-color-scheme: dark)").matches
      ? "quell-dark"
      : "quell-light";
  }
  return pref;
}

// --- Tab navigation ---

function nextTab(): void {
  if (sessions.length <= 1) return;
  const idx = sessions.findIndex((s) => s.id === activeSessionId);
  const next = (idx + 1) % sessions.length;
  switchToSession(sessions[next].id);
}

function prevTab(): void {
  if (sessions.length <= 1) return;
  const idx = sessions.findIndex((s) => s.id === activeSessionId);
  const prev = (idx - 1 + sessions.length) % sessions.length;
  switchToSession(sessions[prev].id);
}

function switchToTabIndex(index: number): void {
  if (index >= 0 && index < sessions.length) {
    switchToSession(sessions[index].id);
  }
}

function openShortcuts(): void {
  const overlay = document.getElementById("shortcuts-overlay")!;
  const grid = overlay.querySelector(".shortcuts-grid")!;
  grid.innerHTML = "";
  const shortcuts: [string, string][] = [
    ["Ctrl+Shift+P", "Command palette"],
    ["Ctrl+Shift+F", "Search in terminal"],
    ["Ctrl+Shift+N", "New tab"],
    ["Ctrl+Tab", "Next tab"],
    ["Ctrl+Shift+Tab", "Previous tab"],
    ["Ctrl+1-9", "Jump to tab"],
    ["Ctrl+= / Ctrl+-", "Zoom in / out"],
    ["Ctrl+0", "Reset zoom"],
    ["F3 / Shift+F3", "Next / previous match"],
    ["Ctrl+C", "Copy (with selection) / SIGINT"],
    ["Ctrl+V", "Paste"],
    ["Ctrl+Shift+C/V", "Alternative copy / paste"],
    ["Win+H", "Voice typing"],
  ];
  for (const [key, desc] of shortcuts) {
    const kbd = document.createElement("kbd");
    kbd.textContent = key;
    const span = document.createElement("span");
    span.textContent = desc;
    grid.appendChild(kbd);
    grid.appendChild(span);
  }
  overlay.hidden = false;
}

function closeShortcuts(): void {
  document.getElementById("shortcuts-overlay")!.hidden = true;
}

function removeActiveSession(): void {
  if (activeSessionId) {
    removeSession(activeSessionId);
  }
}

/// Restart the active session in-place with new args (preserves tab position and name).
async function restartActiveSession(newArgs: string): Promise<void> {
  const session = sessions.find((s) => s.id === activeSessionId);
  if (!session) return;

  // Tear down the running session
  disconnectSession(session.id, session.instance.terminal);
  try {
    await closeSession(session.id);
  } catch {
    // May have already exited
  }

  // Reset terminal and respawn with new args
  session.instance.terminal.reset();
  session.args = newArgs;
  session.exited = false;
  session.streaming = false;
  if (session.streamingTimer) clearTimeout(session.streamingTimer);

  const outputCallback = () => handleSessionOutput(session.id);

  try {
    await spawnSession(
      session.instance.terminal,
      session.instance.terminal.cols,
      session.instance.terminal.rows,
      session.id,
      session.cwd,
      newArgs,
      outputCallback,
    );
    connectSession(session.id, session.instance.terminal, session.cwd, () => currentArgs, () => {
      session.exited = true;
      session.streaming = false;
      if (session.streamingTimer) clearTimeout(session.streamingTimer);
      updateActivityIndicators(session.id);
    }, outputCallback);
  } catch (e) {
    session.instance.terminal.writeln(`\r\n\x1b[31m${String(e)}\x1b[0m`);
    session.exited = true;
  }
}

// --- Initialization ---

document.addEventListener("DOMContentLoaded", async () => {
  const prefs = await loadPreferences();
  currentFontSize = prefs.fontSize;
  currentThemePref = prefs.themePref;
  currentArgs = prefs.defaultArgs;

  // Set UI scale from persisted font size
  const scale = currentFontSize / DEFAULTS.fontSize;
  document.documentElement.style.setProperty("--q-ui-scale", String(scale));

  const themeName = resolveThemeName(currentThemePref);
  applyTheme(themeName);

  // Initialize overlay UIs
  initSearchUI();
  initPaletteUI();

  // Confirm before closing window with running sessions
  getCurrentWindow().onCloseRequested(async (event) => {
    const runningSessions = sessions.filter((s) => !s.exited);
    if (runningSessions.length > 0) {
      const msg = runningSessions.length === 1
        ? "Close the active session?"
        : `Close ${runningSessions.length} active sessions?`;
      const confirmed = await confirm(msg, { title: "Quell" });
      if (!confirmed) {
        event.preventDefault();
      }
    }
  });

  // Shortcuts reference overlay — dismiss with Escape or backdrop click
  const shortcutsOverlay = document.getElementById("shortcuts-overlay")!;
  shortcutsOverlay.addEventListener("click", (e) => {
    if (e.target === shortcutsOverlay) closeShortcuts();
  });
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && !shortcutsOverlay.hidden) {
      closeShortcuts();
      e.stopPropagation();
    }
  });

  // Global key handler — intercepts browser/WebView2 defaults before they fire.
  // Must preventDefault here because xterm's attachCustomKeyEventHandler
  // only works when the terminal textarea is focused.
  document.addEventListener("keydown", (e) => {
    // Ctrl+Shift combos that conflict with browser defaults
    if (e.ctrlKey && e.shiftKey) {
      switch (e.key) {
        case "P": // browser print
          e.preventDefault();
          togglePalette();
          return;
        case "F": // browser find
          e.preventDefault();
          toggleSearch();
          return;
        case "N": // browser incognito
          e.preventDefault();
          addSession();
          return;
        case "C": // browser devtools inspector
          e.preventDefault();
          return;
        case "V": // browser paste-without-formatting
          e.preventDefault();
          return;
      }
    }

    // Ctrl+=/Ctrl+-/Ctrl+0 — prevent browser zoom, let xterm handler manage
    if (e.ctrlKey && !e.shiftKey && !e.altKey) {
      if (e.key === "=" || e.key === "+" || e.key === "-" || e.key === "0") {
        e.preventDefault();
      }
    }

    // F3/Shift+F3 — prevent browser find, handle search navigation
    if (e.key === "F3") {
      e.preventDefault();
      if (e.shiftKey) {
        findPrevious();
      } else {
        findNext();
      }
    }
  });

  // Register palette actions
  const themeActions = getAllThemes().map((t) => ({
    id: `theme-${t.name}`,
    label: t.displayName,
    category: "Theme",
    swatches: [
      t.terminal.background,
      t.chrome.accent,
      t.terminal.red,
      t.terminal.green,
      t.terminal.blue,
    ],
    icon: () =>
      currentThemePref === t.name
        ? { char: "\u2713", color: t.chrome.accent }
        : undefined,
    preview: () => previewTheme(t.name),
    revertPreview: () => previewTheme(resolveThemeName(currentThemePref)),
    execute: () => {
      applyTheme(t.name);
      savePreference("themePref", t.name);
    },
  }));

  registerActions([
    {
      id: "new-tab",
      label: "New Tab",
      shortcut: "Ctrl+Shift+N",
      category: "Tabs",
      execute: () => addSession(),
    },
    {
      id: "new-tab-folder",
      label: "New Tab (Choose Folder)",
      category: "Tabs",
      execute: () => addSessionWithPicker(),
    },
    {
      id: "close-tab",
      label: "Close Tab",
      category: "Tabs",
      execute: removeActiveSession,
    },
    {
      id: "next-tab",
      label: "Next Tab",
      shortcut: "Ctrl+Tab",
      category: "Tabs",
      execute: nextTab,
    },
    {
      id: "prev-tab",
      label: "Previous Tab",
      shortcut: "Ctrl+Shift+Tab",
      category: "Tabs",
      execute: prevTab,
    },
    {
      id: "zoom-in",
      label: "Zoom In",
      shortcut: "Ctrl+=",
      category: "View",
      execute: () => setFontSize(currentFontSize + FONT_STEP),
    },
    {
      id: "zoom-out",
      label: "Zoom Out",
      shortcut: "Ctrl+-",
      category: "View",
      execute: () => setFontSize(currentFontSize - FONT_STEP),
    },
    {
      id: "zoom-reset",
      label: "Reset Zoom",
      shortcut: "Ctrl+0",
      category: "View",
      execute: () => setFontSize(DEFAULTS.fontSize),
    },
    {
      id: "toggle-search",
      label: "Find in Terminal",
      shortcut: "Ctrl+Shift+F",
      category: "Search",
      execute: toggleSearch,
    },
    {
      id: "toggle-skip-permissions",
      label: () =>
        currentArgs.includes("--dangerously-skip-permissions")
          ? "Disable --dangerously-skip-permissions"
          : "Enable --dangerously-skip-permissions",
      icon: () =>
        currentArgs.includes("--dangerously-skip-permissions")
          ? { char: "\u26a0", color: "#e0944a" }
          : undefined,
      category: "Session",
      execute: async () => {
        const flag = "--dangerously-skip-permissions";
        const enabling = !currentArgs.includes(flag);
        const newArgs = enabling
          ? (currentArgs ? `${currentArgs} ${flag}` : flag)
          : currentArgs.replace(flag, "").trim();

        const hasActiveSession = sessions.some(
          (s) => s.id === activeSessionId && !s.exited,
        );

        if (hasActiveSession) {
          const action = enabling ? "enable" : "disable";
          const confirmed = await confirm(
            `This will ${action} --dangerously-skip-permissions and restart the active session. Continue?`,
            { title: "Quell" },
          );
          if (!confirmed) return;
        }

        currentArgs = newArgs;
        savePreference("defaultArgs", currentArgs);

        if (hasActiveSession) {
          await restartActiveSession(currentArgs);
        }
      },
    },
    {
      id: "keyboard-shortcuts",
      label: "Keyboard Shortcuts",
      category: "Help",
      execute: () => openShortcuts(),
    },
    ...themeActions,
  ]);

  // Register keyboard shortcut callbacks
  registerShortcuts({
    nextTab,
    prevTab,
    switchToTab: switchToTabIndex,
    newTab: () => addSession(),
    zoomIn: () => setFontSize(currentFontSize + FONT_STEP),
    zoomOut: () => setFontSize(currentFontSize - FONT_STEP),
    zoomReset: () => setFontSize(DEFAULTS.fontSize),
    toggleSearch,
    togglePalette,
    findNext,
    findPrevious,
  });

  // Initialize global IPC listeners
  await initIpcListeners();

  // Show welcome state — no session until user picks a folder
  showWelcome();

  // New tab button
  document.getElementById("tab-new")!.addEventListener("click", () => {
    addSession();
  });

  // ResizeObserver
  let resizeTimeout: ReturnType<typeof setTimeout> | null = null;
  const resizeObserver = new ResizeObserver(() => {
    if (resizeTimeout) clearTimeout(resizeTimeout);
    resizeTimeout = setTimeout(() => {
      const active = sessions.find((s) => s.id === activeSessionId);
      if (active) {
        active.instance.fitAddon.fit();
      }
    }, 32);
  });
  resizeObserver.observe(getTerminalsEl());

  // Auto dark/light switching at runtime
  const darkMediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
  darkMediaQuery.addEventListener("change", () => {
    if (currentThemePref === "system") {
      applyTheme(resolveThemeName("system"));
    }
  });
});
