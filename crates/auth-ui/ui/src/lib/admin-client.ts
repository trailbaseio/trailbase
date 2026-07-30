import { initClient } from "trailbase";
import type { Client, Tokens } from "trailbase";
import { atom } from "nanostores";

export const $client = atom<Client | undefined>();

type UnknownMessage = {
  type: string;
  value?: unknown;
};

type SetupMessage = {
  type: "setup";
  value: {
    tokens: Tokens | null;
    url?: string;
    theme?: string;
  };
};

type Message = SetupMessage | UnknownMessage;

export function installPostMessageHandler() {
  window.addEventListener("message", (event) => {
    const msg = event.data as Message;

    switch (msg.type) {
      case "setup": {
        const value = msg.value as SetupMessage["value"];
        if (value.theme === "dark") {
          applyTheme(value.theme === "dark" ? "dark" : "light");
        }

        const tokens = value.tokens;
        if (!tokens) {
          console.debug("Received null tokens:", msg);
          break;
        }

        $client.set(initClient(defaultBaseUri(), { tokens }));

        break;
      }
      default: {
        console.warn("Expected setup message, got:", msg);
        break;
      }
    }
  });
}

export function applyTheme(theme: "light" | "dark") {
  const root = document.documentElement;
  root.classList.toggle("dark", theme === "dark");
  root.setAttribute("data-kb-theme", theme);
}

function defaultBaseUri() {
  // console.debug(
  //   "adminClient location:",
  //   window.location,
  //   document.head.baseURI,
  // );

  const href = window.location.href;
  const origin = window.location.origin;

  if (!(href === "about:srcdoc" || origin === "null")) {
    // We're NOT running in a srcdoc iframe.
    return origin;
  }

  // We POSITIVELY are running in a srcdoc iframe.
  //
  // The document's baseURI is typically just the documents href, unless it
  // was explicitly overridden by the admin UI to the server root when
  // running in a dev server.
  const uri = new URL(document.head.baseURI);
  const inDevServer = uri.pathname === "/";
  if (inDevServer) {
    return uri;
  }

  return uri.origin;
}
