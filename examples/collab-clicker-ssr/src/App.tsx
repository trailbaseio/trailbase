import { createSignal, onMount, Match, Switch } from "solid-js"
import { TbFillBrandGithub } from "solid-icons/tb";

import logo from "../public/favicon.svg";

export type InitialData = {
  initialClickCount: number;
  error?: string;
};

declare global {
  interface Window {
    __INITIAL_DATA__: InitialData | null;
  }
}

type ClickedRequest = {
  count: number
};

export function App(props: InitialData) {
  const [count, setCount] = createSignal(props.initialClickCount)

  const onClick = () => {
    setCount((count) => count + 1);

    fetch("/clicked").then(async (response) => {
      const clicked = (await response.json()) as ClickedRequest;
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

      <div class="flex flex-col min-[560px]:flex-row gap-2 justify-center items-center">
        <a
          href="/_/admin?loginMessage=email:%20admin@localhost%20%E2%80%A2%20password:%20secret"
          class={`${buttonStyle} w-[180px] rounded bg-neutral-100`}
        >
          Admin Dashboard
        </a>

        <ul class="list-none">
          <li>login: <span class="font-semibold">admin@localhost</span></li>
          <li>password: <span class="font-semibold">secret</span></li>
        </ul>

        <a
          href="/_/auth/login"
          class={`${buttonStyle} w-[180px] rounded bg-neutral-100 font-bold`}
        >
          Auth UI
        </a>
      </div>

      <a
        class="flex gap-2 justify-center items-center"
        href="https://github.com/trailbaseio/trailbase"
      >
        <p>If you like the demo, consider leaving a ⭐ on GitHub </p>

        <TbFillBrandGithub size={20} />
      </a>

      <div>
        <button
          class={`${buttonStyle} rounded-full`}
          onClick={onClick}
        >
          <img class="size-[256px] m-2" src={logo} />
        </button>
      </div>

      <div class="flex justify-center">
        <Switch>
          <Match when={props.error !== undefined}>
            <div class={`${cardStyle} bg-red-200 text-lg font-bold`}>
              Error: {props.error}
            </div>
          </Match>

          <Match when={true}>
            <button class={`${buttonStyle} w-[200px] rounded bg-neutral-100`} onClick={onClick}>
              {count()}x clicked globally
            </button>
          </Match>
        </Switch>
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
    </div >
  )
}

const cardStyle = "m-2 p-4 outline outline-1 outline-natural-200 rounded text-sm max-w-[680px]";
const buttonStyle = "p-2 scale-95 hover:scale-100 hover:bg-accent-200 active:scale-90 animate-all font-bold";
