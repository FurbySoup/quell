import { Terminal, ITheme } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { SearchAddon } from "@xterm/addon-search";
import { open } from "@tauri-apps/plugin-shell";
import { Unicode11Addon } from "@xterm/addon-unicode11";
import "@xterm/xterm/css/xterm.css";

export interface TerminalInstance {
  terminal: Terminal;
  fitAddon: FitAddon;
  searchAddon: SearchAddon;
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
  toggleSearch: () => void;
  togglePalette: () => void;
  findNext: () => void;
  findPrevious: () => void;
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
  theme: ITheme,
  fontSize?: number,
  scrollback?: number,
): TerminalInstance {
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

    // Ctrl+Shift shortcuts
    // F/P/N are handled by the global document keydown handler to prevent
    // browser defaults. We return false here to stop xterm from processing them.
    if (event.ctrlKey && event.shiftKey) {
      switch (event.key) {
        case "C":
          copySelection();
          return false;
        case "V":
          pasteFromClipboard(event);
          return false;
        case "F":
        case "P":
        case "N":
          return false;
      }
    }

    // F3 / Shift+F3 — action handled by global handler, block from terminal
    if (event.key === "F3") {
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

  // Addon loading order: WebGL (rendering) -> Unicode11 (text) -> WebLinks (UX) -> Search -> Fit (layout)
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
      try {
        const url = new URL(uri);
        if (url.protocol === "https:" || url.protocol === "http:") {
          open(uri);
        }
      } catch {
        // Invalid URL, ignore
      }
    }),
  );

  const searchAddon = new SearchAddon();
  terminal.loadAddon(searchAddon);

  const fitAddon = new FitAddon();
  terminal.loadAddon(fitAddon);
  fitAddon.fit();

  return { terminal, fitAddon, searchAddon, container, sessionId };
}

export function activateTerminal(instance: TerminalInstance): void {
  instance.container.classList.add("active");
  instance.fitAddon.fit();
  instance.terminal.focus();
}

export function deactivateTerminal(instance: TerminalInstance): void {
  instance.container.classList.remove("active");
}
