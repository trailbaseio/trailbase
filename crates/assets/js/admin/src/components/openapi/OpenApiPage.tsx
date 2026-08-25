import { onMount, JSX } from "solid-js";

// Import with side-effects for the custom web component.
import "rapidoc";

import { adminFetch } from "@/lib/fetch";
import { createTheme } from "@/lib/theme";
import { $tokens } from "@/lib/client";

declare module "solid-js" {
  // eslint-disable-next-line @typescript-eslint/no-namespace
  namespace JSX {
    interface IntrinsicElements {
      "rapi-doc": JSX.HTMLAttributes<HTMLElement> & {
        "spec-url"?: string;
        theme?: string;
        // https://github.com/rapi-doc/RapiDoc/blob/7f53d25959e5a4e1beb4b610aaef445b896838f2/src/rapidoc.js#L47
        //
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        [key: string]: any;
      };
    }
  }
}

type RapiDoc = JSX.IntrinsicElements["rapi-doc"];

export default function Page() {
  let ref: RapiDoc | undefined;

  const theme = createTheme();

  onMount(async () => {
    const res = await adminFetch("/openapi.json");
    const spec = await res.json();

    if (!ref) {
      return;
    }

    // Lazy load the actual OpenApi spec.
    ref.loadSpec(spec);

    const url = serverUrl();
    if (url) {
      ref.setAttribute("server-url", serverUrl());
      ref.setAttribute("default-api-server", serverUrl());
    }

    ref.addEventListener("spec-loaded", () => {
      // HACK: Fix `api-info` style, which has a margin-left of -15px.
      const apiInfo = ref.shadowRoot.getElementById("api-info");
      if (apiInfo) {
        apiInfo.style.marginLeft = "0";
      }
    });

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    ref.addEventListener("before-try", (e: any) => {
      const tokens = $tokens.get();
      if (tokens) {
        e.detail.request.headers.append(
          "Authorization",
          `Bearer ${tokens.auth_token}`,
        );
        e.detail.request.headers.append("Refresh-Token", tokens.refresh_token);
        e.detail.request.headers.append("CSRF-Token", tokens.csrf_token);
      }
    });
  });

  return (
    <rapi-doc
      ref={
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        ref as any
      }
      load-fonts="false"
      sort-tags="true"
      theme={theme()} // "light" | "dark"
      bg-color={theme() === "light" ? "#FFFFFF" : "#09090B"}
      primary-color={primary}
      render-style="view" // "read" | "view" | "focused"
      layout="row" // "row" | "column"
      schema-style="table" // "tree" | "table"
      show-header="false" // removes the top bar: logo + title
      allow-try="true"
      persist-auth="false"
      allow-authentication="false"
      allow-server-selection="false"
    >
      {/* Contents */}
      <div class="m-4" />
    </rapi-doc>
  );
}

const serverUrl = () =>
  import.meta.env.DEV ? "http://localhost:4000" : undefined;
const primary = "#0073a8" as const;
