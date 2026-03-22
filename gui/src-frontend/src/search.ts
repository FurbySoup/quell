import type { SearchAddon } from "@xterm/addon-search";

let currentAddon: SearchAddon | null = null;
let searchQuery = "";
let searchOptions = { regex: false, caseSensitive: false, wholeWord: false };

const overlay = () => document.getElementById("search-overlay")!;
const input = () => document.getElementById("search-input") as HTMLInputElement;
const countEl = () => document.getElementById("search-count")!;

let isOpen = false;

export function setActiveSearchAddon(addon: SearchAddon): void {
  currentAddon = addon;
  // Re-run search if overlay is open and there's a query
  if (isOpen && searchQuery) {
    currentAddon.findNext(searchQuery, {
      ...searchOptions,
      incremental: true,
    });
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
    if (currentAddon && searchQuery) {
      currentAddon.findNext(searchQuery, {
        ...searchOptions,
        incremental: true,
      });
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
    if (currentAddon && searchQuery) {
      currentAddon.findNext(searchQuery, {
        ...searchOptions,
        incremental: true,
      });
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
  if (currentAddon && searchQuery) {
    currentAddon.findNext(searchQuery, searchOptions);
  }
}

export function findPrevious(): void {
  if (currentAddon && searchQuery) {
    currentAddon.findPrevious(searchQuery, searchOptions);
  }
}
