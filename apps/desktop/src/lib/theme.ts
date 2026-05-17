export type ThemePreference = "system" | "light" | "dark";

const STORAGE_KEY = "fathom:theme";

export function readStoredPreference(): ThemePreference {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    if (v === "light" || v === "dark" || v === "system") return v;
  } catch {
    // localStorage may be unavailable in sandboxed contexts; fall through.
  }
  return "system";
}

export function storePreference(pref: ThemePreference): void {
  try {
    localStorage.setItem(STORAGE_KEY, pref);
  } catch {
    // ignore
  }
}

export function nextPreference(current: ThemePreference): ThemePreference {
  if (current === "system") return "light";
  if (current === "light") return "dark";
  return "system";
}

export function preferenceLabel(pref: ThemePreference): string {
  if (pref === "light") return "Light";
  if (pref === "dark") return "Dark";
  return "System";
}

export function preferenceGlyph(pref: ThemePreference): string {
  if (pref === "light") return "☼";
  if (pref === "dark") return "☾";
  return "◐";
}

/** Resolve a preference to a concrete theme using the current OS preference. */
export function resolveTheme(pref: ThemePreference): "light" | "dark" {
  if (pref === "light" || pref === "dark") return pref;
  if (typeof window === "undefined" || !window.matchMedia) return "light";
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

export function applyTheme(pref: ThemePreference): void {
  const theme = resolveTheme(pref);
  document.documentElement.setAttribute("data-theme", theme);
}

/**
 * Subscribe to OS-level color-scheme changes. The callback only fires while the
 * user's preference is "system"; returns an unsubscribe function.
 */
export function watchSystemTheme(getPref: () => ThemePreference, onChange: () => void): () => void {
  if (typeof window === "undefined" || !window.matchMedia) return () => {};
  const mq = window.matchMedia("(prefers-color-scheme: dark)");
  const handler = () => {
    if (getPref() === "system") onChange();
  };
  mq.addEventListener("change", handler);
  return () => mq.removeEventListener("change", handler);
}
