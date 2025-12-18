import { createSignal, onMount } from "solid-js"
import logo from "../public/favicon.svg";

export type Clicked = {
  count: number
};

declare global {
  interface Window {
    __INITIAL_DATA__: Clicked | null;
  }
}

export function App({ initialCount }: { initialCount?: number }) {
  const [count, setCount] = createSignal(initialCount ?? 0)

  const onClick = () => {
    setCount((count) => count + 1);

    fetch("/clicked").then(async (response) => {
      const clicked = (await response.json()) as Clicked;
      if (clicked.count > count()) {
        setCount(clicked.count);
      }
    });
  };

  onMount(async () => {
    const trailbase = await import("trailbase");
    const sleep = (ms: number) => new Promise(r => setTimeout(r, ms));

    const listen = async () => {
      const client = trailbase.initClient(window.location.origin);
      const api = client.records("counter");

      const reader = (await api.subscribe(1)).getReader();

      while (true) {
        const { done, value } = await reader.read();
        if (done) {
          console.log("done");
          break;
        }

        const update = value as { Update?: { value?: number } };
        const updatedCount = update.Update?.value;
        if (updatedCount && updatedCount > count()) {
          setCount(updatedCount);
        }
      }
    };

    // Re-connect loop.
    while (true) {
      await listen().catch(console.error)
      await sleep(5000);
    }
  });

  return (
    <div class="flex flex-col gap-4 my-8 text-neutral-800">
      <h1 class="bg-gradient-to-r from-accent-600 via-purple-500 to-pink-500 inline-block text-transparent bg-clip-text">
        TrailBase Demo
      </h1>

      <a href="https://github.com/trailbaseio/trailbase">
        If you like the demo, consider leaving a ⭐ on GitHub
      </a>

      <div>
        <button
          class={`${buttonStyle} rounded-full`}
          onClick={onClick}
        >
          <img class="size-[256px] m-2" src={logo} />
        </button>
      </div>

      <div>
        <button class={`${buttonStyle} w-[200px] rounded bg-neutral-100`} onClick={onClick}>
          {count()}x clicked globally
        </button>
      </div>

      <p>Click the acorn across different tabs, browsers or computers to make everyone's count go 🚀</p>

      <div class={cardStyle}>
        <p class="font-bold py-1">Context</p>
        <p>
          This page showcases TrailBase's "realtime" APIs and server-side rendering (SSR) capabilities.
          The initial page-load contains pre-rendered HTML, which is then hydrated on the client.
          This reduces latency by saving the client a round-trip to fetch the initial counter value.
          The client also subscribes to counter changes and updates whenever
          someone else presses the acorn.
        </p>
      </div>
    </div>
  )
}

const cardStyle = "m-4 p-4 outline outline-1 outline-natural-200 rounded text-sm max-w-[680px]";
const buttonStyle = "p-2 scale-95 hover:scale-100 hover:bg-accent-200 active:scale-90 animate-all font-bold";
