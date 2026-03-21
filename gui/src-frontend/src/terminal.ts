import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import "@xterm/xterm/css/xterm.css";

let fitAddonInstance: FitAddon | null = null;

export function initTerminal(container: HTMLElement): Terminal {
  const terminal = new Terminal({
    fontFamily: "Cascadia Code, Consolas, monospace",
    fontSize: 14,
    cursorBlink: true,
    theme: {
      background: "#1e1e1e",
      foreground: "#d4d4d4",
      cursor: "#d4d4d4",
      selectionBackground: "#264f78",
    },
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
    resizeTimeout = setTimeout(() => fitAddon.fit(), 100);
  });

  return terminal;
}

export function getFitAddon(): FitAddon | null {
  return fitAddonInstance;
}
