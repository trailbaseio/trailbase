import { createSignal, onMount, onCleanup } from "solid-js";
import type { Accessor } from "solid-js";

export function createWindowWidth(): Accessor<number> {
  const [width, setWidth] = createSignal(window.innerWidth);

  const handler = (_event: Event) => setWidth(window.innerWidth);

  onMount(() => window.addEventListener("resize", handler));
  onCleanup(() => window.removeEventListener("resize", handler));

  return width;
}

export function createSetOnce<T>(initial: T): [
  () => T,
  (v: T) => void,
  {
    reset: (v: T) => void;
  },
] {
  let called = false;
  const [v, setV] = createSignal<T>(initial);

  const setter = (v: T) => {
    if (!called) {
      called = true;
      setV(() => v);
    }
  };

  return [v, setter, { reset: setV }];
}
