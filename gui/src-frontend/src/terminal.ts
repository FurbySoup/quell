import { Terminal, ITheme } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
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

/// Callback for paste events — set by ipc.ts with the correct session ID
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

export function createTerminal(
  parent: HTMLElement,
  sessionId: string,
  themeMode?: ThemeMode,
): TerminalInstance {
  const theme = getTheme(themeMode ?? "dark");

  const container = document.createElement("div");
  container.className = "terminal-container";
  container.dataset.sessionId = sessionId;
  parent.appendChild(container);

  const terminal = new Terminal({
    fontFamily: "Cascadia Code, Consolas, monospace",
    fontSize: 14,
    cursorBlink: true,
    theme,
  });

  terminal.open(container);

  // Copy/paste keyboard shortcuts
  terminal.attachCustomKeyEventHandler((event: KeyboardEvent): boolean => {
    if (event.type !== "keydown") {
      return true;
    }

    // Ctrl+C with selection -> copy to clipboard
    if (event.ctrlKey && event.key === "c" && terminal.hasSelection()) {
      navigator.clipboard.writeText(terminal.getSelection());
      return false;
    }

    // Ctrl+V -> paste from clipboard via registered callback
    if (event.ctrlKey && event.key === "v") {
      // preventDefault stops the browser's native paste event from also
      // firing on xterm's textarea, which would cause double paste
      event.preventDefault();
      const cb = pasteCallbacks.get(terminal);
      if (cb) {
        navigator.clipboard.readText().then((text) => {
          if (text) cb(text);
        });
      }
      return false;
    }

    return true;
  });

  const fitAddon = new FitAddon();
  terminal.loadAddon(fitAddon);
  fitAddon.fit();

  try {
    const webglAddon = new WebglAddon();
    terminal.loadAddon(webglAddon);
  } catch (e) {
    console.warn("WebGL addon failed to load, falling back to canvas renderer:", e);
  }

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
