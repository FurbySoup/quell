import { Terminal, ITheme } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { open } from "@tauri-apps/plugin-shell";
import { Unicode11Addon } from "@xterm/addon-unicode11";
import "@xterm/xterm/css/xterm.css";

export type ThemeMode = "dark" | "light";

const darkTheme: ITheme = {
  background: "#1e1e1e",
  foreground: "#d4d4d4",
  cursor: "#d4d4d4",
  selectionBackground: "#264f78",
  black: "#000000",
  red: "#cd3131",
  green: "#0dbc79",
  yellow: "#e5e510",
  blue: "#2472c8",
  magenta: "#bc3fbc",
  cyan: "#11a8cd",
  white: "#e5e5e5",
  brightBlack: "#666666",
  brightRed: "#f14c4c",
  brightGreen: "#23d18b",
  brightYellow: "#f5f543",
  brightBlue: "#3b8eea",
  brightMagenta: "#d670d6",
  brightCyan: "#29b8db",
  brightWhite: "#e5e5e5",
};

const lightTheme: ITheme = {
  background: "#ffffff",
  foreground: "#383a42",
  cursor: "#383a42",
  selectionBackground: "#add6ff",
  black: "#000000",
  red: "#cd3131",
  green: "#008000",
  yellow: "#795e25",
  blue: "#0451a5",
  magenta: "#bc05bc",
  cyan: "#0598bc",
  white: "#a0a0a0",
  brightBlack: "#666666",
  brightRed: "#cd3131",
  brightGreen: "#14ce14",
  brightYellow: "#b5ba00",
  brightBlue: "#0451a5",
  brightMagenta: "#bc05bc",
  brightCyan: "#0598bc",
  brightWhite: "#e5e5e5",
};

export function getTheme(mode: ThemeMode): ITheme {
  return mode === "light" ? lightTheme : darkTheme;
}

export interface TerminalInstance {
  terminal: Terminal;
  fitAddon: FitAddon;
  container: HTMLDivElement;
  sessionId: string;
}

// --- Shortcut callback registry (avoids circular imports with main.ts) ---

export interface ShortcutCallbacks {
  nextTab: () => void;
  prevTab: () => void;
  switchToTab: (index: number) => void;
  newTab: () => void;
  zoomIn: () => void;
  zoomOut: () => void;
  zoomReset: () => void;
}

let shortcuts: ShortcutCallbacks | null = null;

export function registerShortcuts(callbacks: ShortcutCallbacks): void {
  shortcuts = callbacks;
}

// --- Paste callback registry ---

type PasteCallback = (text: string) => void;
const pasteCallbacks = new Map<Terminal, PasteCallback>();

export function setPasteCallback(
  terminal: Terminal,
  callback: PasteCallback,
): void {
  pasteCallbacks.set(terminal, callback);
}

export function clearPasteCallback(terminal: Terminal): void {
  pasteCallbacks.delete(terminal);
}

// --- Terminal creation ---

export function createTerminal(
  parent: HTMLElement,
  sessionId: string,
  themeMode?: ThemeMode,
  fontSize?: number,
  scrollback?: number,
): TerminalInstance {
  const theme = getTheme(themeMode ?? "dark");

  const container = document.createElement("div");
  container.className = "terminal-container";
  container.dataset.sessionId = sessionId;
  parent.appendChild(container);

  const terminal = new Terminal({
    fontFamily: "Cascadia Code, Consolas, monospace",
    fontSize: fontSize ?? 14,
    cursorBlink: true,
    scrollback: scrollback ?? 10000,
    allowProposedApi: true,
    theme,
  });

  terminal.open(container);

  const copySelection = () => {
    if (terminal.hasSelection()) {
      navigator.clipboard.writeText(terminal.getSelection());
    }
  };

  const pasteFromClipboard = (ev: KeyboardEvent) => {
    ev.preventDefault();
    const cb = pasteCallbacks.get(terminal);
    if (cb) {
      navigator.clipboard.readText().then((text) => {
        if (text) cb(text);
      });
    }
  };

  // Keyboard shortcut handler
  terminal.attachCustomKeyEventHandler((event: KeyboardEvent): boolean => {
    if (event.type !== "keydown") {
      return true;
    }

    // Ctrl+C with selection -> copy, without selection -> SIGINT (pass through)
    if (event.ctrlKey && event.key === "c" && terminal.hasSelection()) {
      copySelection();
      return false;
    }

    // Ctrl+V -> paste
    if (event.ctrlKey && event.key === "v") {
      pasteFromClipboard(event);
      return false;
    }

    // Ctrl+Shift+C / Ctrl+Shift+V -> alternative copy/paste
    if (event.ctrlKey && event.shiftKey && event.key === "C") {
      copySelection();
      return false;
    }
    if (event.ctrlKey && event.shiftKey && event.key === "V") {
      pasteFromClipboard(event);
      return false;
    }

    if (shortcuts) {
      // Ctrl+Tab / Ctrl+Shift+Tab -> tab cycling
      if (event.ctrlKey && event.key === "Tab") {
        event.preventDefault();
        if (event.shiftKey) {
          shortcuts.prevTab();
        } else {
          shortcuts.nextTab();
        }
        return false;
      }

      // Ctrl+1-9 -> direct tab switch
      if (event.ctrlKey && !event.shiftKey && !event.altKey) {
        const num = parseInt(event.key);
        if (num >= 1 && num <= 9) {
          shortcuts.switchToTab(num - 1);
          return false;
        }
      }

      // Ctrl+Shift+N -> new tab
      if (event.ctrlKey && event.shiftKey && event.key === "N") {
        shortcuts.newTab();
        return false;
      }

      // Ctrl+= / Ctrl+- / Ctrl+0 -> zoom
      if (event.ctrlKey && !event.shiftKey && !event.altKey) {
        if (event.key === "=" || event.key === "+") {
          event.preventDefault();
          shortcuts.zoomIn();
          return false;
        }
        if (event.key === "-") {
          event.preventDefault();
          shortcuts.zoomOut();
          return false;
        }
        if (event.key === "0") {
          event.preventDefault();
          shortcuts.zoomReset();
          return false;
        }
      }
    }

    return true;
  });

  // Addon loading order: WebGL (rendering) -> Unicode11 (text) -> WebLinks (UX) -> Fit (layout)
  try {
    const webglAddon = new WebglAddon();
    webglAddon.onContextLoss(() => {
      console.warn("WebGL context lost, falling back to canvas renderer");
      webglAddon.dispose();
    });
    terminal.loadAddon(webglAddon);
  } catch (e) {
    console.warn(
      "WebGL addon failed to load, falling back to canvas renderer:",
      e,
    );
  }

  const unicode11Addon = new Unicode11Addon();
  terminal.loadAddon(unicode11Addon);
  terminal.unicode.activeVersion = "11";

  terminal.loadAddon(
    new WebLinksAddon((_event, uri) => {
      open(uri);
    }),
  );

  const fitAddon = new FitAddon();
  terminal.loadAddon(fitAddon);
  fitAddon.fit();

  return { terminal, fitAddon, container, sessionId };
}

export function activateTerminal(instance: TerminalInstance): void {
  instance.container.classList.add("active");
  instance.fitAddon.fit();
  instance.terminal.focus();
}

export function deactivateTerminal(instance: TerminalInstance): void {
  instance.container.classList.remove("active");
}
