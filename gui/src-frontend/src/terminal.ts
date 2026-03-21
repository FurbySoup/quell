import { Terminal, ITheme } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import "@xterm/xterm/css/xterm.css";

let fitAddonInstance: FitAddon | null = null;

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

export interface TerminalSize {
  cols: number;
  rows: number;
}

export function initTerminal(
  container: HTMLElement,
  onResize?: (size: TerminalSize) => void,
  themeMode?: ThemeMode,
): Terminal {
  const theme = getTheme(themeMode ?? "dark");

  const terminal = new Terminal({
    fontFamily: "Cascadia Code, Consolas, monospace",
    fontSize: 14,
    cursorBlink: true,
    theme,
  });

  terminal.open(container);

  const fitAddon = new FitAddon();
  fitAddonInstance = fitAddon;
  terminal.loadAddon(fitAddon);
  fitAddon.fit();

  try {
    const webglAddon = new WebglAddon();
    terminal.loadAddon(webglAddon);
  } catch (e) {
    console.warn("WebGL addon failed to load, falling back to canvas renderer:", e);
  }

  let resizeTimeout: ReturnType<typeof setTimeout> | null = null;
  window.addEventListener("resize", () => {
    if (resizeTimeout) clearTimeout(resizeTimeout);
    resizeTimeout = setTimeout(() => {
      fitAddon.fit();
      if (onResize) {
        onResize({ cols: terminal.cols, rows: terminal.rows });
      }
    }, 100);
  });

  return terminal;
}

export function getFitAddon(): FitAddon | null {
  return fitAddonInstance;
}
