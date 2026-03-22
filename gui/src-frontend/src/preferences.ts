import { Store } from "@tauri-apps/plugin-store";

export interface Preferences {
  fontSize: number;
  themePref: "system" | "dark" | "light";
}

export const DEFAULTS: Preferences = {
  fontSize: 14,
  themePref: "system",
};

let store: Store | null = null;

async function getStore(): Promise<Store> {
  if (!store) {
    store = await Store.load("preferences.json");
  }
  return store;
}

export async function loadPreferences(): Promise<Preferences> {
  try {
    const s = await getStore();
    return {
      fontSize: (await s.get<number>("fontSize")) ?? DEFAULTS.fontSize,
      themePref:
        (await s.get<Preferences["themePref"]>("themePref")) ??
        DEFAULTS.themePref,
    };
  } catch {
    return { ...DEFAULTS };
  }
}

export async function savePreference<K extends keyof Preferences>(
  key: K,
  value: Preferences[K],
): Promise<void> {
  try {
    const s = await getStore();
    await s.set(key, value);
    await s.save();
  } catch (e) {
    console.warn("Failed to save preference:", key, e);
  }
}
