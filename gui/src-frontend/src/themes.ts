import type { ITheme } from "@xterm/xterm";

export interface QuellTheme {
  name: string;
  displayName: string;
  kind: "dark" | "light";
  chrome: {
    bg: string;
    bgSecondary: string;
    border: string;
    text: string;
    textMuted: string;
    accent: string;
    scrollbarTrack: string;
    scrollbarThumb: string;
    scrollbarThumbHover: string;
    overlayBg: string;
    overlayBorder: string;
    inputBg: string;
    inputBorder: string;
    inputBorderFocus: string;
    highlight: string;
  };
  terminal: {
    background: string;
    foreground: string;
    cursor: string;
    selectionBackground: string;
    black: string;
    red: string;
    green: string;
    yellow: string;
    blue: string;
    magenta: string;
    cyan: string;
    white: string;
    brightBlack: string;
    brightRed: string;
    brightGreen: string;
    brightYellow: string;
    brightBlue: string;
    brightMagenta: string;
    brightCyan: string;
    brightWhite: string;
  };
}

// ---------------------------------------------------------------------------
//  1. QUELL DARK — Blue-tinted dark with copper/amber accent
//     Signature theme. Steel-blue base, warm copper highlights.
// ---------------------------------------------------------------------------
const quellDark: QuellTheme = {
  name: "quell-dark",
  displayName: "Quell Dark",
  kind: "dark",
  chrome: {
    bg: "#0d1117",
    bgSecondary: "#161b22",
    border: "#30363d",
    text: "#c9d1d9",
    textMuted: "#8b949e",
    accent: "#d4843e",
    scrollbarTrack: "#0d1117",
    scrollbarThumb: "#30363d",
    scrollbarThumbHover: "#484f58",
    overlayBg: "#161b22",
    overlayBorder: "#30363d",
    inputBg: "#0d1117",
    inputBorder: "#30363d",
    inputBorderFocus: "#d4843e",
    highlight: "rgba(212, 132, 62, 0.3)",
  },
  terminal: {
    background: "#0d1117",
    foreground: "#c9d1d9",
    cursor: "#d4843e",
    selectionBackground: "#264f78",
    black: "#0d1117",
    red: "#da6771",
    green: "#7bc67e",
    yellow: "#d4a853",
    blue: "#6cb6ff",
    magenta: "#c490d4",
    cyan: "#63c2b5",
    white: "#c9d1d9",
    brightBlack: "#484f58",
    brightRed: "#f47067",
    brightGreen: "#85e089",
    brightYellow: "#e3b341",
    brightBlue: "#79c0ff",
    brightMagenta: "#d2a8ff",
    brightCyan: "#76d9cb",
    brightWhite: "#f0f6fc",
  },
};

// ---------------------------------------------------------------------------
//  2. QUELL LIGHT — Warm parchment tones, dark ink text
//     Paper-white with sepia undertone, copper accents carried from dark.
// ---------------------------------------------------------------------------
const quellLight: QuellTheme = {
  name: "quell-light",
  displayName: "Quell Light",
  kind: "light",
  chrome: {
    bg: "#faf8f5",
    bgSecondary: "#f0ece6",
    border: "#d8d3cb",
    text: "#2e2a25",
    textMuted: "#77716a",
    accent: "#b06a2a",
    scrollbarTrack: "#faf8f5",
    scrollbarThumb: "#c8c3bb",
    scrollbarThumbHover: "#a9a49c",
    overlayBg: "#f0ece6",
    overlayBorder: "#d8d3cb",
    inputBg: "#faf8f5",
    inputBorder: "#d8d3cb",
    inputBorderFocus: "#b06a2a",
    highlight: "rgba(176, 106, 42, 0.2)",
  },
  terminal: {
    background: "#faf8f5",
    foreground: "#2e2a25",
    cursor: "#b06a2a",
    selectionBackground: "#d4c9b8",
    black: "#2e2a25",
    red: "#c4392a",
    green: "#347a38",
    yellow: "#8a6b20",
    blue: "#2a5fad",
    magenta: "#8b3daa",
    cyan: "#1a7d72",
    white: "#d8d3cb",
    brightBlack: "#77716a",
    brightRed: "#d94d3e",
    brightGreen: "#3e9243",
    brightYellow: "#a07b25",
    brightBlue: "#3570c6",
    brightMagenta: "#a24ec5",
    brightCyan: "#219588",
    brightWhite: "#faf8f5",
  },
};

// ---------------------------------------------------------------------------
//  3. HIGH CONTRAST — Maximum readability, WCAG AAA
//     Near-black background, near-white text, vivid saturated ANSI.
// ---------------------------------------------------------------------------
const highContrast: QuellTheme = {
  name: "high-contrast",
  displayName: "High Contrast",
  kind: "dark",
  chrome: {
    bg: "#000000",
    bgSecondary: "#0a0a0a",
    border: "#6e6e6e",
    text: "#ffffff",
    textMuted: "#b0b0b0",
    accent: "#ffcc00",
    scrollbarTrack: "#000000",
    scrollbarThumb: "#6e6e6e",
    scrollbarThumbHover: "#909090",
    overlayBg: "#0a0a0a",
    overlayBorder: "#6e6e6e",
    inputBg: "#000000",
    inputBorder: "#6e6e6e",
    inputBorderFocus: "#ffcc00",
    highlight: "rgba(255, 204, 0, 0.35)",
  },
  terminal: {
    background: "#000000",
    foreground: "#ffffff",
    cursor: "#ffcc00",
    selectionBackground: "#3a5fcd",
    black: "#000000",
    red: "#ff5555",
    green: "#55ff55",
    yellow: "#ffff55",
    blue: "#5588ff",
    magenta: "#ff55ff",
    cyan: "#55ffff",
    white: "#ffffff",
    brightBlack: "#808080",
    brightRed: "#ff7777",
    brightGreen: "#77ff77",
    brightYellow: "#ffff77",
    brightBlue: "#77aaff",
    brightMagenta: "#ff77ff",
    brightCyan: "#77ffff",
    brightWhite: "#ffffff",
  },
};

// ---------------------------------------------------------------------------
//  4. SOLARIZED DARK — Ethan Schoonover's classic
// ---------------------------------------------------------------------------
const solarizedDark: QuellTheme = {
  name: "solarized-dark",
  displayName: "Solarized Dark",
  kind: "dark",
  chrome: {
    bg: "#002b36",
    bgSecondary: "#073642",
    border: "#586e75",
    text: "#839496",
    textMuted: "#657b83",
    accent: "#b58900",
    scrollbarTrack: "#002b36",
    scrollbarThumb: "#586e75",
    scrollbarThumbHover: "#657b83",
    overlayBg: "#073642",
    overlayBorder: "#586e75",
    inputBg: "#002b36",
    inputBorder: "#586e75",
    inputBorderFocus: "#b58900",
    highlight: "rgba(181, 137, 0, 0.3)",
  },
  terminal: {
    background: "#002b36",
    foreground: "#839496",
    cursor: "#839496",
    selectionBackground: "#073642",
    black: "#073642",
    red: "#dc322f",
    green: "#859900",
    yellow: "#b58900",
    blue: "#268bd2",
    magenta: "#d33682",
    cyan: "#2aa198",
    white: "#eee8d5",
    brightBlack: "#586e75",
    brightRed: "#cb4b16",
    brightGreen: "#93a1a1",
    brightYellow: "#657b83",
    brightBlue: "#839496",
    brightMagenta: "#6c71c4",
    brightCyan: "#93a1a1",
    brightWhite: "#fdf6e3",
  },
};

// ---------------------------------------------------------------------------
//  5. SOLARIZED LIGHT
// ---------------------------------------------------------------------------
const solarizedLight: QuellTheme = {
  name: "solarized-light",
  displayName: "Solarized Light",
  kind: "light",
  chrome: {
    bg: "#fdf6e3",
    bgSecondary: "#eee8d5",
    border: "#93a1a1",
    text: "#657b83",
    textMuted: "#93a1a1",
    accent: "#b58900",
    scrollbarTrack: "#fdf6e3",
    scrollbarThumb: "#93a1a1",
    scrollbarThumbHover: "#839496",
    overlayBg: "#eee8d5",
    overlayBorder: "#93a1a1",
    inputBg: "#fdf6e3",
    inputBorder: "#93a1a1",
    inputBorderFocus: "#b58900",
    highlight: "rgba(181, 137, 0, 0.2)",
  },
  terminal: {
    background: "#fdf6e3",
    foreground: "#657b83",
    cursor: "#657b83",
    selectionBackground: "#eee8d5",
    black: "#073642",
    red: "#dc322f",
    green: "#859900",
    yellow: "#b58900",
    blue: "#268bd2",
    magenta: "#d33682",
    cyan: "#2aa198",
    white: "#eee8d5",
    brightBlack: "#586e75",
    brightRed: "#cb4b16",
    brightGreen: "#93a1a1",
    brightYellow: "#657b83",
    brightBlue: "#839496",
    brightMagenta: "#6c71c4",
    brightCyan: "#93a1a1",
    brightWhite: "#fdf6e3",
  },
};

// ---------------------------------------------------------------------------
//  6. MONOKAI — Classic warm dark theme
// ---------------------------------------------------------------------------
const monokai: QuellTheme = {
  name: "monokai",
  displayName: "Monokai",
  kind: "dark",
  chrome: {
    bg: "#272822",
    bgSecondary: "#2e2f2a",
    border: "#49483e",
    text: "#f8f8f2",
    textMuted: "#a6a28c",
    accent: "#a6e22e",
    scrollbarTrack: "#272822",
    scrollbarThumb: "#49483e",
    scrollbarThumbHover: "#5e5d50",
    overlayBg: "#2e2f2a",
    overlayBorder: "#49483e",
    inputBg: "#272822",
    inputBorder: "#49483e",
    inputBorderFocus: "#a6e22e",
    highlight: "rgba(166, 226, 46, 0.25)",
  },
  terminal: {
    background: "#272822",
    foreground: "#f8f8f2",
    cursor: "#f8f8f2",
    selectionBackground: "#49483e",
    black: "#272822",
    red: "#f92672",
    green: "#a6e22e",
    yellow: "#f4bf75",
    blue: "#66d9ef",
    magenta: "#ae81ff",
    cyan: "#a1efe4",
    white: "#f8f8f2",
    brightBlack: "#75715e",
    brightRed: "#f92672",
    brightGreen: "#a6e22e",
    brightYellow: "#f4bf75",
    brightBlue: "#66d9ef",
    brightMagenta: "#ae81ff",
    brightCyan: "#a1efe4",
    brightWhite: "#f9f8f5",
  },
};

// ---------------------------------------------------------------------------
//  7. NORD — Arctic, cool-toned
// ---------------------------------------------------------------------------
const nord: QuellTheme = {
  name: "nord",
  displayName: "Nord",
  kind: "dark",
  chrome: {
    bg: "#2e3440",
    bgSecondary: "#3b4252",
    border: "#4c566a",
    text: "#d8dee9",
    textMuted: "#9aa5b4",
    accent: "#88c0d0",
    scrollbarTrack: "#2e3440",
    scrollbarThumb: "#4c566a",
    scrollbarThumbHover: "#5e6779",
    overlayBg: "#3b4252",
    overlayBorder: "#4c566a",
    inputBg: "#2e3440",
    inputBorder: "#4c566a",
    inputBorderFocus: "#88c0d0",
    highlight: "rgba(136, 192, 208, 0.25)",
  },
  terminal: {
    background: "#2e3440",
    foreground: "#d8dee9",
    cursor: "#d8dee9",
    selectionBackground: "#4c566a",
    black: "#3b4252",
    red: "#bf616a",
    green: "#a3be8c",
    yellow: "#ebcb8b",
    blue: "#81a1c1",
    magenta: "#b48ead",
    cyan: "#88c0d0",
    white: "#e5e9f0",
    brightBlack: "#4c566a",
    brightRed: "#bf616a",
    brightGreen: "#a3be8c",
    brightYellow: "#ebcb8b",
    brightBlue: "#81a1c1",
    brightMagenta: "#b48ead",
    brightCyan: "#8fbcbb",
    brightWhite: "#eceff4",
  },
};

// ---------------------------------------------------------------------------
//  8. CVD-FRIENDLY — Blue-orange palette, no red-green reliance
//     Safe for deuteranopia, protanopia, and tritanopia.
// ---------------------------------------------------------------------------
const cvdFriendly: QuellTheme = {
  name: "cvd-friendly",
  displayName: "CVD-Friendly",
  kind: "dark",
  chrome: {
    bg: "#1a1b26",
    bgSecondary: "#24253a",
    border: "#3d3f5c",
    text: "#c0caf5",
    textMuted: "#8289a8",
    accent: "#e0944a",
    scrollbarTrack: "#1a1b26",
    scrollbarThumb: "#3d3f5c",
    scrollbarThumbHover: "#545778",
    overlayBg: "#24253a",
    overlayBorder: "#3d3f5c",
    inputBg: "#1a1b26",
    inputBorder: "#3d3f5c",
    inputBorderFocus: "#e0944a",
    highlight: "rgba(224, 148, 74, 0.3)",
  },
  terminal: {
    background: "#1a1b26",
    foreground: "#c0caf5",
    cursor: "#e0944a",
    selectionBackground: "#33467c",
    black: "#1a1b26",
    red: "#e0944a",       // orange instead of red
    green: "#7aa2f7",     // blue instead of green
    yellow: "#e8c46c",    // gold/amber
    blue: "#7aa2f7",
    magenta: "#bb9af7",   // purple (safe)
    cyan: "#73c7d4",      // teal (safe)
    white: "#c0caf5",
    brightBlack: "#545778",
    brightRed: "#f0a85c",
    brightGreen: "#89b4fa",
    brightYellow: "#f2d280",
    brightBlue: "#89b4fa",
    brightMagenta: "#cba6ff",
    brightCyan: "#89d8e4",
    brightWhite: "#e0e6ff",
  },
};

// ---------------------------------------------------------------------------
//  9. DRACULA — Purple-infused dark
// ---------------------------------------------------------------------------
const dracula: QuellTheme = {
  name: "dracula",
  displayName: "Dracula",
  kind: "dark",
  chrome: {
    bg: "#282a36",
    bgSecondary: "#2d2f3d",
    border: "#44475a",
    text: "#f8f8f2",
    textMuted: "#9a9bae",
    accent: "#bd93f9",
    scrollbarTrack: "#282a36",
    scrollbarThumb: "#44475a",
    scrollbarThumbHover: "#565970",
    overlayBg: "#2d2f3d",
    overlayBorder: "#44475a",
    inputBg: "#282a36",
    inputBorder: "#44475a",
    inputBorderFocus: "#bd93f9",
    highlight: "rgba(189, 147, 249, 0.25)",
  },
  terminal: {
    background: "#282a36",
    foreground: "#f8f8f2",
    cursor: "#f8f8f2",
    selectionBackground: "#44475a",
    black: "#21222c",
    red: "#ff5555",
    green: "#50fa7b",
    yellow: "#f1fa8c",
    blue: "#bd93f9",
    magenta: "#ff79c6",
    cyan: "#8be9fd",
    white: "#f8f8f2",
    brightBlack: "#6272a4",
    brightRed: "#ff6e6e",
    brightGreen: "#69ff94",
    brightYellow: "#ffffa5",
    brightBlue: "#d6acff",
    brightMagenta: "#ff92df",
    brightCyan: "#a4ffff",
    brightWhite: "#ffffff",
  },
};

// ---------------------------------------------------------------------------
//  10. TOKYO NIGHT — Soft dark with vibrant blues and purples
// ---------------------------------------------------------------------------
const tokyoNight: QuellTheme = {
  name: "tokyo-night",
  displayName: "Tokyo Night",
  kind: "dark",
  chrome: {
    bg: "#1a1b26",
    bgSecondary: "#1f2335",
    border: "#3b4261",
    text: "#c0caf5",
    textMuted: "#737aa2",
    accent: "#7aa2f7",
    scrollbarTrack: "#1a1b26",
    scrollbarThumb: "#3b4261",
    scrollbarThumbHover: "#545c7e",
    overlayBg: "#1f2335",
    overlayBorder: "#3b4261",
    inputBg: "#1a1b26",
    inputBorder: "#3b4261",
    inputBorderFocus: "#7aa2f7",
    highlight: "rgba(122, 162, 247, 0.25)",
  },
  terminal: {
    background: "#1a1b26",
    foreground: "#c0caf5",
    cursor: "#c0caf5",
    selectionBackground: "#33467c",
    black: "#15161e",
    red: "#f7768e",
    green: "#9ece6a",
    yellow: "#e0af68",
    blue: "#7aa2f7",
    magenta: "#bb9af7",
    cyan: "#7dcfff",
    white: "#a9b1d6",
    brightBlack: "#414868",
    brightRed: "#f7768e",
    brightGreen: "#9ece6a",
    brightYellow: "#e0af68",
    brightBlue: "#7aa2f7",
    brightMagenta: "#bb9af7",
    brightCyan: "#7dcfff",
    brightWhite: "#c0caf5",
  },
};

// ---------------------------------------------------------------------------
//  11. CATPPUCCIN MOCHA — Soothing pastel dark
// ---------------------------------------------------------------------------
const catppuccinMocha: QuellTheme = {
  name: "catppuccin-mocha",
  displayName: "Catppuccin Mocha",
  kind: "dark",
  chrome: {
    bg: "#1e1e2e",
    bgSecondary: "#252536",
    border: "#45475a",
    text: "#cdd6f4",
    textMuted: "#9399b2",
    accent: "#cba6f7",
    scrollbarTrack: "#1e1e2e",
    scrollbarThumb: "#45475a",
    scrollbarThumbHover: "#585b70",
    overlayBg: "#252536",
    overlayBorder: "#45475a",
    inputBg: "#1e1e2e",
    inputBorder: "#45475a",
    inputBorderFocus: "#cba6f7",
    highlight: "rgba(203, 166, 247, 0.25)",
  },
  terminal: {
    background: "#1e1e2e",
    foreground: "#cdd6f4",
    cursor: "#f5e0dc",
    selectionBackground: "#45475a",
    black: "#45475a",
    red: "#f38ba8",
    green: "#a6e3a1",
    yellow: "#f9e2af",
    blue: "#89b4fa",
    magenta: "#cba6f7",
    cyan: "#94e2d5",
    white: "#bac2de",
    brightBlack: "#585b70",
    brightRed: "#f38ba8",
    brightGreen: "#a6e3a1",
    brightYellow: "#f9e2af",
    brightBlue: "#89b4fa",
    brightMagenta: "#cba6f7",
    brightCyan: "#94e2d5",
    brightWhite: "#a6adc8",
  },
};

// ---------------------------------------------------------------------------
//  12. GRUVBOX DARK — Retro warm browns and oranges
// ---------------------------------------------------------------------------
const gruvboxDark: QuellTheme = {
  name: "gruvbox-dark",
  displayName: "Gruvbox Dark",
  kind: "dark",
  chrome: {
    bg: "#282828",
    bgSecondary: "#3c3836",
    border: "#504945",
    text: "#ebdbb2",
    textMuted: "#a89984",
    accent: "#fe8019",
    scrollbarTrack: "#282828",
    scrollbarThumb: "#504945",
    scrollbarThumbHover: "#665c54",
    overlayBg: "#3c3836",
    overlayBorder: "#504945",
    inputBg: "#282828",
    inputBorder: "#504945",
    inputBorderFocus: "#fe8019",
    highlight: "rgba(254, 128, 25, 0.25)",
  },
  terminal: {
    background: "#282828",
    foreground: "#ebdbb2",
    cursor: "#ebdbb2",
    selectionBackground: "#504945",
    black: "#282828",
    red: "#cc241d",
    green: "#98971a",
    yellow: "#d79921",
    blue: "#458588",
    magenta: "#b16286",
    cyan: "#689d6a",
    white: "#a89984",
    brightBlack: "#928374",
    brightRed: "#fb4934",
    brightGreen: "#b8bb26",
    brightYellow: "#fabd2f",
    brightBlue: "#83a598",
    brightMagenta: "#d3869b",
    brightCyan: "#8ec07c",
    brightWhite: "#ebdbb2",
  },
};

// ---------------------------------------------------------------------------
//  13. ONE DARK — Atom's iconic theme
// ---------------------------------------------------------------------------
const oneDark: QuellTheme = {
  name: "one-dark",
  displayName: "One Dark",
  kind: "dark",
  chrome: {
    bg: "#282c34",
    bgSecondary: "#2c313a",
    border: "#3e4451",
    text: "#abb2bf",
    textMuted: "#7f848e",
    accent: "#61afef",
    scrollbarTrack: "#282c34",
    scrollbarThumb: "#3e4451",
    scrollbarThumbHover: "#4f5666",
    overlayBg: "#2c313a",
    overlayBorder: "#3e4451",
    inputBg: "#282c34",
    inputBorder: "#3e4451",
    inputBorderFocus: "#61afef",
    highlight: "rgba(97, 175, 239, 0.25)",
  },
  terminal: {
    background: "#282c34",
    foreground: "#abb2bf",
    cursor: "#528bff",
    selectionBackground: "#3e4451",
    black: "#282c34",
    red: "#e06c75",
    green: "#98c379",
    yellow: "#e5c07b",
    blue: "#61afef",
    magenta: "#c678dd",
    cyan: "#56b6c2",
    white: "#abb2bf",
    brightBlack: "#5c6370",
    brightRed: "#e06c75",
    brightGreen: "#98c379",
    brightYellow: "#e5c07b",
    brightBlue: "#61afef",
    brightMagenta: "#c678dd",
    brightCyan: "#56b6c2",
    brightWhite: "#ffffff",
  },
};

// ---------------------------------------------------------------------------
//  14. ROSÉ PINE — Muted, elegant dark with rose accents
// ---------------------------------------------------------------------------
const rosePine: QuellTheme = {
  name: "rose-pine",
  displayName: "Ros\u00e9 Pine",
  kind: "dark",
  chrome: {
    bg: "#191724",
    bgSecondary: "#1f1d2e",
    border: "#403d52",
    text: "#e0def4",
    textMuted: "#908caa",
    accent: "#ebbcba",
    scrollbarTrack: "#191724",
    scrollbarThumb: "#403d52",
    scrollbarThumbHover: "#524f67",
    overlayBg: "#1f1d2e",
    overlayBorder: "#403d52",
    inputBg: "#191724",
    inputBorder: "#403d52",
    inputBorderFocus: "#ebbcba",
    highlight: "rgba(235, 188, 186, 0.25)",
  },
  terminal: {
    background: "#191724",
    foreground: "#e0def4",
    cursor: "#524f67",
    selectionBackground: "#403d52",
    black: "#26233a",
    red: "#eb6f92",
    green: "#9ccfd8",
    yellow: "#f6c177",
    blue: "#31748f",
    magenta: "#c4a7e7",
    cyan: "#9ccfd8",
    white: "#e0def4",
    brightBlack: "#6e6a86",
    brightRed: "#eb6f92",
    brightGreen: "#9ccfd8",
    brightYellow: "#f6c177",
    brightBlue: "#31748f",
    brightMagenta: "#c4a7e7",
    brightCyan: "#9ccfd8",
    brightWhite: "#e0def4",
  },
};

// ---------------------------------------------------------------------------
//  Registry
// ---------------------------------------------------------------------------

const THEMES: QuellTheme[] = [
  quellDark,
  quellLight,
  highContrast,
  solarizedDark,
  solarizedLight,
  monokai,
  nord,
  dracula,
  tokyoNight,
  catppuccinMocha,
  gruvboxDark,
  oneDark,
  rosePine,
  cvdFriendly,
];

export function getAllThemes(): QuellTheme[] {
  return THEMES;
}

export function getThemeByName(name: string): QuellTheme {
  return THEMES.find((t) => t.name === name) ?? quellDark;
}

export function toXtermTheme(theme: QuellTheme): ITheme {
  return { ...theme.terminal };
}

export function applyQuellTheme(theme: QuellTheme): void {
  const root = document.documentElement;

  // Set all chrome CSS custom properties
  const chromeProps: Record<string, string> = {
    "--q-bg": theme.chrome.bg,
    "--q-bg-secondary": theme.chrome.bgSecondary,
    "--q-border": theme.chrome.border,
    "--q-text": theme.chrome.text,
    "--q-text-muted": theme.chrome.textMuted,
    "--q-accent": theme.chrome.accent,
    "--q-scrollbar-track": theme.chrome.scrollbarTrack,
    "--q-scrollbar-thumb": theme.chrome.scrollbarThumb,
    "--q-scrollbar-thumb-hover": theme.chrome.scrollbarThumbHover,
    "--q-overlay-bg": theme.chrome.overlayBg,
    "--q-overlay-border": theme.chrome.overlayBorder,
    "--q-input-bg": theme.chrome.inputBg,
    "--q-input-border": theme.chrome.inputBorder,
    "--q-input-border-focus": theme.chrome.inputBorderFocus,
    "--q-highlight": theme.chrome.highlight,
  };

  for (const [prop, value] of Object.entries(chromeProps)) {
    root.style.setProperty(prop, value);
  }

  // Set body background directly (some elements read it before CSS vars resolve)
  document.body.style.background = theme.chrome.bg;

  // Preserve UI scale if already set
  if (!root.style.getPropertyValue("--q-ui-scale")) {
    root.style.setProperty("--q-ui-scale", "1");
  }

  // Set kind class for edge cases
  document.body.classList.toggle("light", theme.kind === "light");
  document.body.classList.toggle("dark", theme.kind === "dark");
}
