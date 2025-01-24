import { createSignal, onMount } from "solid-js"

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
      const client = new trailbase.Client(window.location.origin);
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
    <div class="flex flex-col gap-4 text-neutral-800">
      <h1>TrailBase Clicker</h1>

      <div class="px-4 py-2">
        <button
          class="rounded bg-neutral-100 p-2 font-medium hover:scale-110 hover:outline outline-accent-600 active:scale-100 animate-all"
          onClick={onClick}
        >
          clicked {count()} times
        </button>
      </div>

      <p>
        Click the button across different tabs, windows or browsers.
      </p>
    </div>
  )
}
