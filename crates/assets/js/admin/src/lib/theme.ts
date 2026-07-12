import { persistentAtom } from "@nanostores/persistent";

export type ThemePreference = "light" | "dark" | "system";
type ResolvedTheme = "light" | "dark";

function decodeThemePreference(value: string): ThemePreference {
  if (value === "light" || value === "dark" || value === "system") {
    return value;
  }
  return "system";
}

export const $themePreference = persistentAtom<ThemePreference>(
  "theme_preference",
  "system",
  {
    encode: (value) => value,
    decode: decodeThemePreference,
  },
);

function prefersSystemDarkMode(): boolean {
  return (
    typeof window !== "undefined" &&
    typeof window.matchMedia === "function" &&
    window.matchMedia(DARK_MODE_QUERY).matches
  );
}

export function resolveThemePreference(
  preference: ThemePreference,
): ResolvedTheme {
  if (preference === "system") {
    return prefersSystemDarkMode() ? "dark" : "light";
  }
  return preference;
}

function applyResolvedTheme(theme: ResolvedTheme) {
  if (typeof document === "undefined") {
    return;
  }

  const root = document.documentElement;
  root.classList.toggle("dark", theme === "dark");
  root.setAttribute("data-kb-theme", theme);
}

export function currentTheme(): ResolvedTheme {
  const root = document.documentElement;
  return root.classList.contains("dark") ? "dark" : "light";
}

export function applyThemePreference(preference: ThemePreference) {
  applyResolvedTheme(resolveThemePreference(preference));
}

export function listenForSystemThemeChanges(
  onChange: () => void,
): (() => void) | undefined {
  if (
    typeof window === "undefined" ||
    typeof window.matchMedia !== "function"
  ) {
    return undefined;
  }

  const media = window.matchMedia(DARK_MODE_QUERY);
  media.addEventListener("change", onChange);

  return () => media.removeEventListener("change", onChange);
}

const DARK_MODE_QUERY = "(prefers-color-scheme: dark)";
