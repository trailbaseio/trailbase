import { onMount, JSX } from "solid-js";
import type { Tokens } from "trailbase";
import { TbOutlineInfoCircle } from "solid-icons/tb";

// Import with side-effects for the custom web component.
import "rapidoc";

import { adminFetch } from "@/lib/fetch";
import { createTheme } from "@/lib/theme";
import { $tokens } from "@/lib/client";

import {
  TextField,
  TextFieldLabel,
  TextFieldInput,
} from "@/components/ui/text-field";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";

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
  let tokensRef: HTMLInputElement | undefined;

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
      function getTokens(): Tokens | null {
        const inputTokens = tokensRef?.value;
        if (inputTokens) {
          return JSON.parse(atob(inputTokens)) as Tokens;
        }

        return $tokens.get();
      }

      const tokens = getTokens();
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
      <div class="mx-4 my-4 lg:mx-8">
        <TextField class="flex items-center gap-2">
          <TextFieldLabel>
            <Tooltip>
              <TooltipTrigger class="flex gap-2">
                <span class="underline">Tokens:</span>
                <TbOutlineInfoCircle class="inline-block" />
              </TooltipTrigger>

              <TooltipContent>
                You can optionally provide explicit tokens to impersonate
                another user. To get the tokens of verified, non-admin users,
                click a user on the accounts page.
              </TooltipContent>
            </Tooltip>
          </TextFieldLabel>

          <TextFieldInput
            ref={tokensRef}
            type="password"
            autocomplete="new-password"
          />
        </TextField>
      </div>
    </rapi-doc>
  );
}

const serverUrl = () =>
  import.meta.env.DEV ? "http://localhost:4000" : undefined;
const primary = "#0073a8" as const;
