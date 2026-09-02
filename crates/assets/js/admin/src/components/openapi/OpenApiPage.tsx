import { onMount, JSX, createSignal } from "solid-js";
import type { Tokens } from "trailbase";

// Import with side-effects for the custom web component.
import "rapidoc";

import { adminFetch } from "@/lib/fetch";
import { createTheme } from "@/lib/theme";
import { $tokens } from "@/lib/client";

import { Button } from "@/components/ui/button";
import { Header } from "@/components/Header";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
  usePopoverContext,
} from "@/components/ui/popover";
import { TextField, TextFieldInput } from "@/components/ui/text-field";

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

  const [version, setVersion] = createSignal<string | undefined>();
  const [tokens, setTokens] = createSignal<string>("");
  const theme = createTheme();

  onMount(async () => {
    const res = await adminFetch("/openapi.json");
    const spec = await res.json();

    if (!ref) {
      return;
    }

    // Remove info to keep output clean.
    const info = spec["info"];
    spec["info"] = {};
    setVersion(info["version"]);

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
        const inputTokens = tokens();
        if (inputTokens) {
          return JSON.parse(atob(inputTokens)) as Tokens;
        }

        return $tokens.get();
      }

      const inputTokens = getTokens();
      if (inputTokens) {
        e.detail.request.headers.append(
          "Authorization",
          `Bearer ${inputTokens.auth_token}`,
        );
        e.detail.request.headers.append(
          "Refresh-Token",
          inputTokens.refresh_token,
        );
        e.detail.request.headers.append("CSRF-Token", inputTokens.csrf_token);
      }
    });
  });

  return (
    <>
      <Header
        title="OpenApi Explorer"
        description={version()}
        right={
          <Popover id="test">
            <PopoverTrigger as={Button<"button">} variant="outline">
              Tokens
            </PopoverTrigger>

            <PopoverContent class="ui-expanded:shadow-md">
              <TokenPopoverContent tokens={tokens} setTokens={setTokens} />
            </PopoverContent>
          </Popover>
        }
      />

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
      </rapi-doc>
    </>
  );
}

function TokenPopoverContent(props: {
  tokens: () => string;
  setTokens: (v: string) => void;
}) {
  const context = usePopoverContext();

  return (
    <div class="flex flex-col gap-4">
      <p class="text-xs">
        You can optionally provide explicit tokens to impersonate another user.
        To get the tokens of a verified, non-admin user, click the "cookie"
        button in the user details on the accounts page or use the CLI.
      </p>

      <TextField>
        <TextFieldInput
          type="text"
          autocomplete="new-password"
          value={props.tokens()}
          onChange={() => {
            console.debug("close");
            context.close();
          }}
          onInput={(e) => {
            const value = (e.target as HTMLInputElement).value;
            if (value) {
              props.setTokens(value);
            } else {
              props.setTokens("");
            }
          }}
        />
      </TextField>
    </div>
  );
}

const serverUrl = () =>
  import.meta.env.DEV ? "http://localhost:4000" : undefined;
const primary = "#0073a8" as const;
