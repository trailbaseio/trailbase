import { createSignal, onMount, onCleanup } from "solid-js";
import type { JSXElement, Accessor } from "solid-js";
import { persistentAtom } from "@nanostores/persistent";
import { useStore } from "@nanostores/solid";

import {
  Resizable,
  ResizablePanel,
  ResizableHandle,
} from "@/components/ui/resizable";

export function createWindowWidth(): Accessor<number> {
  const [width, setWidth] = createSignal(window.innerWidth);

  const handler = (_event: Event) => setWidth(window.innerWidth);

  onMount(() => window.addEventListener("resize", handler));
  onCleanup(() => window.removeEventListener("resize", handler));

  return width;
}

function setSizes(v: number[] | ((prev: number[]) => number[])) {
  const prev = $sizes.get();
  const next: number[] = typeof v === "function" ? v(prev) : v;
  const width = window.innerWidth;

  // This is a bit hacky. On destruction Corvu pops panes and removes sizes one by one.
  // So switching between pages we'd always start with empty sizes. We basically just avoid
  // shrinking the array. We also make sure the new relative dimension for element[0] is
  // within range.
  if (
    next.length >= prev.length &&
    next[0] >= minSizePx / width &&
    next[0] < maxSizePx / width
  ) {
    return $sizes.set(next);
  }
  return prev;
}

export function SplitView(props: {
  first: (props: { horizontal: boolean }) => JSXElement;
  second: (props: { horizontal: boolean }) => JSXElement;
}) {
  function VerticalSplit() {
    return (
      <div class="flex flex-col overflow-hidden">
        <props.first horizontal={false} />
        <props.second horizontal={false} />
      </div>
    );
  }

  function HorizontalSplit() {
    const size = useStore($sizes);

    return (
      <Resizable
        class="h-dvh w-full overflow-hidden"
        sizes={size()}
        onSizesChange={setSizes}
        orientation="horizontal"
      >
        <ResizablePanel>
          <props.first horizontal={true} />
        </ResizablePanel>

        <ResizableHandle withHandle={true} />

        <ResizablePanel class="grow overflow-x-hidden">
          <props.second horizontal={true} />
        </ResizablePanel>
      </Resizable>
    );
  }

  const windowWidth = createWindowWidth();
  const thresh = 5 * minSizePx;
  return (
    <>{windowWidth() < thresh ? <VerticalSplit /> : <HorizontalSplit />}</>
  );
}

const minSizePx = 160;
const maxSizePx = 300;

function initialSize(): number[] {
  const width = window.innerWidth;
  const left = Math.max(minSizePx, 0.15 * width);
  const right = width - left;

  return [left / width, right / width];
}

export const $sizes = persistentAtom<number[]>(
  "resizable-sizes",
  initialSize(),
  {
    encode: JSON.stringify,
    decode: JSON.parse,
  },
);
