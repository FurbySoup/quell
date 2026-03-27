export interface PaletteAction {
  id: string;
  label: string | (() => string);
  shortcut?: string;
  category?: string;
  swatches?: string[];
  icon?: { char: string; color: string } | (() => { char: string; color: string } | undefined);
  preview?: () => void;
  revertPreview?: () => void;
  execute: () => void;
}

const actions: PaletteAction[] = [];
let isOpen = false;
let selectedIndex = 0;
let filteredActions: PaletteAction[] = [];
let usingKeyboard = false;
let lastPreviewedAction: PaletteAction | null = null;
let previewTimer: ReturnType<typeof setTimeout> | null = null;
const PREVIEW_DEBOUNCE_MS = 80;

const overlayEl = () => document.getElementById("palette-overlay")!;
const inputEl = () =>
  document.getElementById("palette-input") as HTMLInputElement;
const resultsEl = () => document.getElementById("palette-results")!;

export function registerActions(newActions: PaletteAction[]): void {
  actions.push(...newActions);
}

export function openPalette(): void {
  isOpen = true;
  selectedIndex = 0;
  const input = inputEl();
  input.value = "";
  const overlay = overlayEl();
  const palette = overlay.querySelector(".palette") as HTMLElement;
  palette.classList.remove("entering");
  overlay.hidden = false;
  // RAF ensures the browser has painted the visible-but-no-animation frame
  // before we add the class that triggers the animation
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      palette.classList.add("entering");
    });
  });
  input.focus();
  filterAndRender("");
}

export function closePalette(): void {
  if (previewTimer) {
    clearTimeout(previewTimer);
    previewTimer = null;
  }
  if (lastPreviewedAction?.revertPreview) {
    lastPreviewedAction.revertPreview();
  }
  lastPreviewedAction = null;
  isOpen = false;
  const overlay = overlayEl();
  const palette = overlay.querySelector(".palette") as HTMLElement;
  palette.classList.remove("entering");
  palette.classList.add("exiting");
  palette.addEventListener("animationend", () => {
    palette.classList.remove("exiting");
    overlay.hidden = true;
  }, { once: true });
}

export function togglePalette(): void {
  if (isOpen) {
    closePalette();
  } else {
    openPalette();
  }
}

export function isPaletteOpen(): boolean {
  return isOpen;
}

function resolveLabel(action: PaletteAction): string {
  return typeof action.label === "function" ? action.label() : action.label;
}

function fuzzyScore(query: string, label: string): number {
  if (!query) return 1;
  const q = query.toLowerCase();
  const l = label.toLowerCase();
  const idx = l.indexOf(q);
  if (idx === -1) return -1;
  if (idx === 0) return 100;
  if (l[idx - 1] === " " || l[idx - 1] === ":") return 80;
  return 50;
}

function filterAndRender(query: string): void {
  if (!query) {
    filteredActions = [...actions];
  } else {
    filteredActions = actions
      .map((a) => ({ action: a, score: fuzzyScore(query, resolveLabel(a)) }))
      .filter((r) => r.score > 0)
      .sort((a, b) => b.score - a.score)
      .map((r) => r.action);
  }

  selectedIndex = Math.min(selectedIndex, Math.max(0, filteredActions.length - 1));
  renderResults();
}

function renderResults(): void {
  const container = resultsEl();
  container.innerHTML = "";

  if (filteredActions.length === 0) {
    const empty = document.createElement("div");
    empty.className = "palette-empty";
    empty.textContent = "No matching commands";
    container.appendChild(empty);
    return;
  }

  const query = inputEl().value;
  const showHeaders = !query;
  let lastCategory = "";

  for (let i = 0; i < filteredActions.length; i++) {
    const action = filteredActions[i];

    if (showHeaders && action.category && action.category !== lastCategory) {
      lastCategory = action.category;
      const header = document.createElement("div");
      header.className = "palette-category-header";
      header.textContent = action.category;
      container.appendChild(header);
    }

    const item = document.createElement("div");
    item.className = "palette-item" + (i === selectedIndex ? " selected" : "");

    // Active indicator — accent left border instead of inline checkmark
    const resolvedIcon = typeof action.icon === "function" ? action.icon() : action.icon;
    if (resolvedIcon) {
      item.classList.add("active-indicator");
      item.style.borderLeftColor = resolvedIcon.color;
    }

    // Label on the left
    const labelSpan = document.createElement("span");
    if (!showHeaders && action.category) {
      const catSpan = document.createElement("span");
      catSpan.className = "category";
      catSpan.textContent = action.category + ": ";
      labelSpan.appendChild(catSpan);
    }
    labelSpan.appendChild(document.createTextNode(resolveLabel(action)));
    item.appendChild(labelSpan);

    // Right side: spacer pushes shortcut + swatches to the end
    if (action.shortcut) {
      const shortcutKbd = document.createElement("kbd");
      shortcutKbd.className = "shortcut";
      shortcutKbd.textContent = action.shortcut;
      item.appendChild(shortcutKbd);
    }

    if (action.swatches && action.swatches.length > 0) {
      const swatchContainer = document.createElement("span");
      swatchContainer.className = "palette-swatches";
      for (const color of action.swatches) {
        const swatch = document.createElement("span");
        swatch.className = "palette-swatch";
        swatch.style.backgroundColor = color;
        swatchContainer.appendChild(swatch);
      }
      item.appendChild(swatchContainer);
    }

    item.addEventListener("click", () => {
      if (previewTimer) { clearTimeout(previewTimer); previewTimer = null; }
      lastPreviewedAction = null;
      closePalette();
      action.execute();
    });

    item.addEventListener("mouseenter", () => {
      if (!usingKeyboard) {
        selectedIndex = i;
        updateSelection();
      }
    });

    item.addEventListener("mousemove", () => {
      usingKeyboard = false;
    });

    container.appendChild(item);
  }
}

function updateSelection(): void {
  const items = resultsEl().querySelectorAll(".palette-item");
  items.forEach((item, i) => {
    item.classList.toggle("selected", i === selectedIndex);
  });
  items[selectedIndex]?.scrollIntoView({ block: "nearest" });

  // Debounced live preview
  const action = filteredActions[selectedIndex];
  if (action?.preview) {
    if (previewTimer) clearTimeout(previewTimer);
    previewTimer = setTimeout(() => {
      action.preview!();
      lastPreviewedAction = action;
    }, PREVIEW_DEBOUNCE_MS);
  }
}

export function initPaletteUI(): void {
  const input = inputEl();

  input.addEventListener("input", () => {
    selectedIndex = 0;
    filterAndRender(input.value);
  });

  input.addEventListener("keydown", (e) => {
    if (e.key === "Tab") {
      e.preventDefault();
      return;
    }

    if (e.key === "Escape") {
      closePalette();
      return;
    }

    if (e.key === "ArrowDown") {
      e.preventDefault();
      usingKeyboard = true;
      if (selectedIndex < filteredActions.length - 1) {
        selectedIndex++;
        updateSelection();
      }
      return;
    }

    if (e.key === "ArrowUp") {
      e.preventDefault();
      usingKeyboard = true;
      if (selectedIndex > 0) {
        selectedIndex--;
        updateSelection();
      }
      return;
    }

    if (e.key === "Enter") {
      e.preventDefault();
      if (filteredActions[selectedIndex]) {
        if (previewTimer) { clearTimeout(previewTimer); previewTimer = null; }
        lastPreviewedAction = null;
        closePalette();
        filteredActions[selectedIndex].execute();
      }
      return;
    }
  });

  // Click backdrop to close
  overlayEl().addEventListener("click", (e) => {
    if (e.target === overlayEl()) {
      closePalette();
    }
  });
}
