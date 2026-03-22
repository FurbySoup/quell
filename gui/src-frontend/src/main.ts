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
import { open as openDialog } from "@tauri-apps/plugin-dialog";
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
  welcome.innerHTML = "";

  const content = document.createElement("div");
  content.className = "welcome-content";

  const title = document.createElement("h1");
  title.textContent = "> Quell";
  content.appendChild(title);

  const subtitle = document.createElement("p");
  subtitle.className = "welcome-subtitle";
  subtitle.textContent = "Terminal for AI CLI tools";
  content.appendChild(subtitle);

  const btn = document.createElement("button");
  btn.className = "welcome-btn";
  btn.textContent = "Open Folder";
  btn.addEventListener("click", () => {
    addSession();
  });
  content.appendChild(btn);

  const hint = document.createElement("p");
  hint.className = "welcome-hint";
  hint.textContent = "Choose a project folder to start a session";
  content.appendChild(hint);

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

  await spawnSession(
    instance.terminal,
    instance.terminal.cols,
    instance.terminal.rows,
    sessionId,
    cwd,
  );

  const tabEl = createTabElement(sessionId, sessions.length + 1);
  tabsEl.appendChild(tabEl);

  const session: Session = {
    id: sessionId,
    instance,
    tabEl,
    customName: false,
    cwd: cwd!,
  };
  sessions.push(session);

  connectSession(sessionId, instance.terminal, cwd);

  switchToSession(sessionId);
  updateTabBarVisibility();
}

async function removeSession(sessionId: string): Promise<void> {
  const idx = sessions.findIndex((s) => s.id === sessionId);
  if (idx === -1) return;

  const session = sessions[idx];

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

function removeActiveSession(): void {
  if (activeSessionId) {
    removeSession(activeSessionId);
  }
}

// --- Initialization ---

document.addEventListener("DOMContentLoaded", async () => {
  const prefs = await loadPreferences();
  currentFontSize = prefs.fontSize;
  currentThemePref = prefs.themePref;

  // Set UI scale from persisted font size
  const scale = currentFontSize / DEFAULTS.fontSize;
  document.documentElement.style.setProperty("--q-ui-scale", String(scale));

  const themeName = resolveThemeName(currentThemePref);
  applyTheme(themeName);

  // Initialize overlay UIs
  initSearchUI();
  initPaletteUI();

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
