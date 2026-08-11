import { onMount } from "solid-js";
import "rapidoc";

import { adminFetch } from "@/lib/fetch";
import { createTheme } from "@/lib/theme";

declare module "solid-js" {
  // eslint-disable-next-line @typescript-eslint/no-namespace
  namespace JSX {
    interface IntrinsicElements {
      "rapi-doc": JSX.HTMLAttributes<HTMLElement> & {
        "spec-url"?: string;
        theme?: string;
        "render-style"?: string;
        [key: string]: any;
      };
    }
  }
}

export default function Page() {
  let ref: HTMLElement | undefined;
  const theme = createTheme();

  onMount(async () => {
    const res = await adminFetch("/openapi.json");
    const spec = await res.json();

    // RapiDoc API for loading spec late.
    (ref as any)?.loadSpec(spec);
  });

  return (
    <rapi-doc
      ref={ref}
      load-fonts="false"
      theme={theme()} // "light" | "dark"
      render-style="view" // "read" | "view" | "focused"
      layout="row" // "row" | "column"
      schema-style="table" // "tree" | "table"
      show-header="false" // removes the top header bar entirely (logo/title row)
    >
      {/* Contents */}
      <div class="m-4"></div>
    </rapi-doc>
  );
}
