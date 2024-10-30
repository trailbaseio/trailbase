import { createSignal, onCleanup, onMount } from "solid-js";
import type { Accessor } from "solid-js";

export function createDarkMode(): Accessor<boolean> {
  const isDark = () => document.documentElement.dataset["theme"] === "dark";

  const [darkMode, setDarkMode] = createSignal<boolean>(isDark());

  let observer: MutationObserver | undefined;

  onMount(() => {
    observer = new MutationObserver((mutations) => {
      mutations.forEach((mu) => {
        if (mu.type === "attributes" && mu.attributeName === "data-theme") {
          setDarkMode(isDark());
        }
      });
    });
    observer.observe(document.documentElement, { attributes: true });
  });
  onCleanup(() => observer?.disconnect());

  return darkMode;
}
