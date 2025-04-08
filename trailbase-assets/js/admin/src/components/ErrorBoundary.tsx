import { ErrorBoundary as SolidErrorBoundary, type JSX } from "solid-js";

import { client } from "@/lib/fetch";
import { Toaster, showToast } from "@/components/ui/toast";
import { Button } from "@/components/ui/button";

export function ErrorBoundary(props: { children: JSX.Element }) {
  // NOTE: the fallback handles errors during component construction. Not
  // errors at runtime, e.g.in a button handler.
  return (
    <SolidErrorBoundary
      fallback={
        import.meta.env.DEV
          ? undefined
          : (err, reset) => {
              return (
                <div class="m-4 flex flex-col gap-4">
                  {`${err}`}

                  <div>
                    <Button onClick={reset}>Reload</Button>
                  </div>

                  <div>
                    <Button onClick={() => client.logout()}>Re-auth</Button>
                  </div>
                </div>
              );
            }
      }
    >
      {props.children}

      <Toaster />
    </SolidErrorBoundary>
  );
}

window.onerror = function (message, url, lineNumber) {
  const description = `${url}:${lineNumber} ${message}`;
  console.error(description);

  showToast({
    title: "Uncaught Error",
    description,
    variant: "error",
  });
};
