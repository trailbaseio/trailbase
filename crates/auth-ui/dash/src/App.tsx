// import { createSignal } from "solid-js";
import { initClientFromCookies, Client } from "trailbase";

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
  const client: Client = await initClientFromCookies();

  console.log("TOKENS: ", client.tokens());
}

test();

export default App;
