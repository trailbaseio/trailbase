import { createSignal, onMount, onCleanup } from "solid-js";
import type { Accessor } from "solid-js";

export function createWindowWidth(): Accessor<number> {
  const [width, setWidth] = createSignal(window.innerWidth);

  const handler = (_event: Event) => setWidth(window.innerWidth);

  onMount(() => window.addEventListener("resize", handler));
  onCleanup(() => window.removeEventListener("resize", handler));

  return width;
}
