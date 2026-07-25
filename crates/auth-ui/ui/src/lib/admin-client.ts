import { isServer } from "solid-js/web";
import { initClient } from "trailbase";
import type { Client, Tokens } from "trailbase";

export function adminClient(base?: string): Client | undefined {
  if (isServer) {
    return;
  }

  const authTokens: string | null = localStorage.getItem("auth_tokens");
  if (authTokens === null) {
    console.debug("adminClient tokens missing");
    return;
  }

  const tokens: Tokens = JSON.parse(authTokens);
  const uri = base ?? defaultBaseUri();
  console.debug(`adminClient(base = ${uri})`);
  return initClient(uri, { tokens });
}

function defaultBaseUri() {
  console.debug(
    "adminClient location:",
    window.location,
    document.head.baseURI,
  );

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
