import { createSignal, onMount, onCleanup } from "solid-js";
import type { Accessor } from "solid-js";

import { MOBILE_BREAKPOINT } from "@/components/ui/sidebar";

export function createWindowSize(): Accessor<[number, number]> {
  const current = (): [number, number] => [
    window.innerWidth,
    window.innerHeight,
  ];
  const [size, setSize] = createSignal(current());
  const update = () => setSize(current());

  onMount(() => window.addEventListener("resize", update));
  onCleanup(() => window.removeEventListener("resize", update));

  return size;
}

export function createIsMobile(): Accessor<boolean> {
  const size = createWindowSize();
  return () => size()[0] < MOBILE_BREAKPOINT;
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
