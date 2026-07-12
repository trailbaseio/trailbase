import { createSignal, onMount, onCleanup } from "solid-js";
import type { Accessor } from "solid-js";
import { persistentAtom } from "@nanostores/persistent";

export type ResolvedTheme = "light" | "dark";

export function applyResolvedTheme(theme: ResolvedTheme) {
  const root = document.documentElement;

  root.classList.toggle("dark", theme === "dark");
  root.setAttribute("data-kb-theme", theme);
  $themePreference.set(theme);
}

export function currentTheme(): ResolvedTheme {
  return document.documentElement.classList.contains("dark") ? "dark" : "light";
}

export function initializeTheme() {
  function systemsPreferredTheme(): ResolvedTheme {
    const DARK_MODE_QUERY = "(prefers-color-scheme: dark)";
    return window.matchMedia(DARK_MODE_QUERY).matches ? "dark" : "light";
  }

  // Set theme based on stored preference (i.e. user selected it before) or
  // system-wide preference.
  applyResolvedTheme($themePreference.get() ?? systemsPreferredTheme());
}

export function createTheme(): Accessor<ResolvedTheme> {
  const [theme, setTheme] = createSignal<ResolvedTheme>(currentTheme());

  const attrObserver = new MutationObserver((mutations) => {
    mutations.forEach((mu) => {
      if (mu.type === "attributes" && mu.attributeName === "class") {
        setTheme(currentTheme());
      }
    });
  });

  onMount(() =>
    attrObserver.observe(document.documentElement, { attributes: true }),
  );
  onCleanup(() => attrObserver.disconnect());

  return theme;
}

export const $themePreference = persistentAtom<ResolvedTheme | undefined>(
  "theme:selected",
  undefined,
  {
    encode: JSON.stringify,
    decode: JSON.parse,
  },
);
