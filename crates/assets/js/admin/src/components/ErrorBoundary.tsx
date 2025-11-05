import {
  ErrorBoundary as SolidErrorBoundary,
  onMount,
  onCleanup,
} from "solid-js";
import type { JSX } from "solid-js";

import { client } from "@/lib/client";
import { Toaster, showToast } from "@/components/ui/toast";
import { Button } from "@/components/ui/button";

export function ErrorBoundary(props: { children: JSX.Element }) {
  installUncaughtErrorHandlers();

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

function installUncaughtErrorHandlers() {
  const handleSyncErrors = (event: ErrorEvent) => {
    console.error("Uncaught error", event);

    showToast({
      title: import.meta.env.DEV ? "Uncaught Error" : "Error",
      description: import.meta.env.DEV
        ? `${event.message}\n${event.filename}:${event.lineno}`
        : event.message,
      variant: "error",
    });

    event.preventDefault();
    event.stopPropagation();
  };

  const handleAsyncErrors = (event: PromiseRejectionEvent) => {
    console.error("Uncaught async error", event);

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const reason: any = event.reason;

    showToast({
      title: import.meta.env.DEV ? "Uncaught Async Error" : "Error",
      description:
        reason instanceof Error
          ? `${reason.name}: ${reason.message}`
          : `${reason}`,
      variant: "error",
    });

    event.preventDefault();
    event.stopPropagation();
  };

  onMount(() => {
    window.addEventListener("error", handleSyncErrors);
    window.addEventListener("unhandledrejection", handleAsyncErrors);
  });

  onCleanup(() => {
    window.removeEventListener("error", handleSyncErrors);
    window.removeEventListener("unhandledrejection", handleAsyncErrors);
  });
}
