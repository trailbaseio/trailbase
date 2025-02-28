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

function setSizes(next: number[]) {
  const prev = $sizes.get();
  const width = window.innerWidth;

  // This is a bit hacky. On destruction Corvu pops panes and removes sizes one by one.
  // So switching between pages we'd always start with empty sizes. We basically just avoid
  // shrinking the array. We also make sure the new relative dimension for element[0] is
  // within range.
  if (next.length >= prev.length && next.length > 0) {
    const min = minSizePx / width;
    const max = maxSizePx / width;
    const first = Math.min(max, Math.max(min, next[0]));

    return $sizes.set([first, ...next.slice(1)]);
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
        <ResizablePanel class="overflow-hidden">
          <props.first horizontal={true} />
        </ResizablePanel>

        <ResizableHandle withHandle={true} />

        <ResizablePanel class="overflow-x-hidden">
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
const maxSizePx = 400;

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
