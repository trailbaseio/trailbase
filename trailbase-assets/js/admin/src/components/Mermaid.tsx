/* eslint-disable solid/no-innerhtml */
import mermaid from "mermaid";
import Panzoom from "@panzoom/panzoom";
import { createUniqueId, onMount, Show, Suspense } from "solid-js";
import { createAsync } from "@solidjs/router";

import { Spinner } from "@/components/Spinner";

async function render(value: string): Promise<string | undefined> {
  try {
    const { svg } = await mermaid.render(createUniqueId(), value);
    return svg;
  } catch (e) {
    console.error(e);
  }
  return undefined;
}

function Svg(props: { class?: string; svg: string | undefined }) {
  let ref: HTMLDivElement | undefined;

  onMount(() => {
    const svgElement = ref?.querySelector("svg");
    if (svgElement) {
      // Initialize Panzoom
      const panzoomInstance = Panzoom(svgElement, {
        maxScale: 5,
        minScale: 0.5,
        step: 0.1,
      });

      // Add mouse wheel zoom
      ref?.addEventListener("wheel", (event) => {
        panzoomInstance.zoomWithWheel(event);
      });
    }
  });

  return (
    <div
      ref={ref}
      class={`h-[calc(100dvh-98px)] ${props.class}`}
      innerHTML={props.svg}
    />
  );
}

export type MermaidProps = {
  class?: string;
  value: string;
};

export function Mermaid(props: MermaidProps) {
  const svg = createAsync(async () => await render(props.value));

  return (
    <Suspense fallback={<Spinner />}>
      <Show when={svg()}>
        <Svg class={props.class} svg={svg()} />
      </Show>
    </Suspense>
  );
}
