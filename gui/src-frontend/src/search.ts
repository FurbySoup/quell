import type { SearchAddon } from "@xterm/addon-search";
import type { QuellTheme } from "./themes";

let currentAddon: SearchAddon | null = null;
let searchQuery = "";
let searchOptions = { regex: false, caseSensitive: false, wholeWord: false };
const decorations = {
  matchBackground: "#515c6a",
  matchBorder: "#74879f",
  matchOverviewRuler: "#515c6a",
  activeMatchBackground: "#515c6a",
  activeMatchBorder: "#f0c674",
  activeMatchColorOverviewRuler: "#f0c674",
};

let activeTheme: QuellTheme | null = null;

export function setSearchTheme(theme: QuellTheme): void {
  activeTheme = theme;
}

function getDecorations() {
  if (!activeTheme) return decorations;
  return {
    matchBackground: activeTheme.chrome.highlight,
    matchBorder: activeTheme.chrome.inputBorder,
    matchOverviewRuler: activeTheme.chrome.highlight,
    activeMatchBackground: activeTheme.chrome.highlight,
    activeMatchBorder: activeTheme.chrome.accent,
    activeMatchColorOverviewRuler: activeTheme.chrome.accent,
  };
}

export function refreshDecorations(): void {
  if (!isOpen || !currentAddon || !searchQuery || !isValidSearch()) return;
  currentAddon.clearDecorations();
  currentAddon.findNext(searchQuery, searchOpts(true));
}

const overlay = () => document.getElementById("search-overlay")!;
const input = () => document.getElementById("search-input") as HTMLInputElement;
const countEl = () => document.getElementById("search-count")!;

let isOpen = false;

function searchOpts(incremental?: boolean) {
  return { ...searchOptions, decorations: getDecorations(), ...(incremental ? { incremental: true } : {}) };
}

function updateCount(found: boolean): void {
  if (!searchQuery) {
    countEl().textContent = "";
  } else if (!found) {
    countEl().textContent = "No results";
  } else {
    countEl().textContent = "";
  }
}

export function setActiveSearchAddon(addon: SearchAddon): void {
  currentAddon = addon;
  if (isOpen && searchQuery && isValidSearch()) {
    const found = currentAddon.findNext(searchQuery, searchOpts(true));
    updateCount(found);
  }
}

export function initSearchUI(): void {
  const regexBtn = document.getElementById("search-regex")!;
  const caseBtn = document.getElementById("search-case")!;
  const wordBtn = document.getElementById("search-word")!;
  const prevBtn = document.getElementById("search-prev")!;
  const nextBtn = document.getElementById("search-next")!;
  const closeBtn = document.getElementById("search-close")!;
  const inputEl = input();

  inputEl.addEventListener("input", () => {
    searchQuery = inputEl.value;
    if (currentAddon && searchQuery && isValidSearch()) {
      const found = currentAddon.findNext(searchQuery, searchOpts(true));
      updateCount(found);
    } else if (currentAddon && !searchQuery) {
      currentAddon.clearDecorations();
      countEl().textContent = "";
    }
  });

  inputEl.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      closeSearch();
    } else if (e.key === "Enter") {
      if (e.shiftKey) {
        findPrevious();
      } else {
        findNext();
      }
    }
  });

  const toggleOption = (
    key: keyof typeof searchOptions,
    btn: HTMLElement,
  ) => {
    searchOptions[key] = !searchOptions[key];
    btn.classList.toggle("active", searchOptions[key]);
    if (currentAddon && searchQuery && isValidSearch()) {
      const found = currentAddon.findNext(searchQuery, searchOpts(true));
      updateCount(found);
    }
  };

  regexBtn.addEventListener("click", () => toggleOption("regex", regexBtn));
  caseBtn.addEventListener("click", () =>
    toggleOption("caseSensitive", caseBtn),
  );
  wordBtn.addEventListener("click", () =>
    toggleOption("wholeWord", wordBtn),
  );

  prevBtn.addEventListener("click", () => findPrevious());
  nextBtn.addEventListener("click", () => findNext());
  closeBtn.addEventListener("click", () => closeSearch());

  // Focus trap — Tab cycles within search bar
  const searchBar = document.querySelector("#search-overlay .search-bar")!;
  searchBar.addEventListener("keydown", (e: Event) => {
    const ke = e as KeyboardEvent;
    if (ke.key !== "Tab") return;
    const focusable = searchBar.querySelectorAll<HTMLElement>("input, button");
    const list = Array.from(focusable);
    const idx = list.indexOf(document.activeElement as HTMLElement);
    if (idx === -1) return;
    ke.preventDefault();
    const next = ke.shiftKey
      ? (idx - 1 + list.length) % list.length
      : (idx + 1) % list.length;
    list[next].focus();
  });
}

export function openSearch(): void {
  isOpen = true;
  overlay().hidden = false;
  const inputEl = input();
  inputEl.value = searchQuery;
  inputEl.focus();
  inputEl.select();
}

export function closeSearch(): void {
  isOpen = false;
  overlay().hidden = true;
  if (currentAddon) {
    currentAddon.clearDecorations();
  }
  countEl().textContent = "";
}

export function toggleSearch(): void {
  if (isOpen) {
    closeSearch();
  } else {
    openSearch();
  }
}

export function isSearchOpen(): boolean {
  return isOpen;
}

export function findNext(): void {
  if (currentAddon && searchQuery && isValidSearch()) {
    const found = currentAddon.findNext(searchQuery, searchOpts());
    updateCount(found);
  }
}

export function findPrevious(): void {
  if (currentAddon && searchQuery && isValidSearch()) {
    const found = currentAddon.findPrevious(searchQuery, searchOpts());
    updateCount(found);
  }
}

function isValidSearch(): boolean {
  if (searchOptions.regex && searchQuery) {
    try {
      new RegExp(searchQuery);
    } catch {
      countEl().textContent = "Invalid regex";
      currentAddon?.clearDecorations();
      return false;
    }
  }
  return true;
}
