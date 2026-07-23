// import { createSignal } from "solid-js";
import { initClientFromCookies, Client, initClient } from "trailbase";
import type { Tokens } from "trailbase";

function App() {
  return (
    <div class="h-full w-full bg-red-200">
      <section id="center">
        <h1>Auth UI</h1>

        <p>
          {window.innerWidth}x{window.innerHeight} (WxH)
        </p>
      </section>
    </div>
  );
}

async function test() {
  console.debug(import.meta.env.DEV, document.cookie, localStorage);
  const authTokens: string | null = localStorage.getItem("auth_tokens");

  let client: Client | UnderlyingDefaultSource;
  if (authTokens !== null) {
    const tokens: Tokens = JSON.parse(authTokens);

    client = initClient(document.head.baseURI, {
      tokens,
    });
  } else {
    client = await initClientFromCookies();
  }

  console.log("TOKENS: ", client.tokens());
}

test();

export default App;
