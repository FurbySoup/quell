export interface PaletteAction {
  id: string;
  label: string;
  shortcut?: string;
  category?: string;
  execute: () => void;
}

const actions: PaletteAction[] = [];
let isOpen = false;
let selectedIndex = 0;
let filteredActions: PaletteAction[] = [];
let usingKeyboard = false;

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
  overlayEl().hidden = false;
  input.focus();
  filterAndRender("");
}

export function closePalette(): void {
  isOpen = false;
  overlayEl().hidden = true;
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

function fuzzyScore(query: string, label: string): number {
  if (!query) return 1;
  const q = query.toLowerCase();
  const l = label.toLowerCase();
  const idx = l.indexOf(q);
  if (idx === -1) return -1;
  // Prefix match scores highest, then word boundary, then substring
  if (idx === 0) return 100;
  if (l[idx - 1] === " " || l[idx - 1] === ":") return 80;
  return 50;
}

function filterAndRender(query: string): void {
  if (!query) {
    filteredActions = [...actions];
  } else {
    filteredActions = actions
      .map((a) => ({ action: a, score: fuzzyScore(query, a.label) }))
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

  for (let i = 0; i < filteredActions.length; i++) {
    const action = filteredActions[i];
    const item = document.createElement("div");
    item.className = "palette-item" + (i === selectedIndex ? " selected" : "");

    const labelSpan = document.createElement("span");
    if (action.category) {
      const catSpan = document.createElement("span");
      catSpan.className = "category";
      catSpan.textContent = action.category + ": ";
      labelSpan.appendChild(catSpan);
    }
    labelSpan.appendChild(document.createTextNode(action.label));
    item.appendChild(labelSpan);

    if (action.shortcut) {
      const shortcutSpan = document.createElement("span");
      shortcutSpan.className = "shortcut";
      shortcutSpan.textContent = action.shortcut;
      item.appendChild(shortcutSpan);
    }

    item.addEventListener("click", () => {
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
  // Scroll selected into view
  items[selectedIndex]?.scrollIntoView({ block: "nearest" });
}

export function initPaletteUI(): void {
  const input = inputEl();

  input.addEventListener("input", () => {
    selectedIndex = 0;
    filterAndRender(input.value);
  });

  input.addEventListener("keydown", (e) => {
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
