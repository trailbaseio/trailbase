import {
  ErrorBoundary as SolidErrorBoundary,
  type JSX,
  children,
} from "solid-js";
import { Toaster, showToast } from "@/components/ui/toast";

export function ErrorBoundary(props: { children: JSX.Element }) {
  const resolved = children(() => props.children);

  // NOTE: the fallback handles errors during component construction. Not
  // errors at runtime, e.g.in a button handler.
  return (
    <SolidErrorBoundary
      fallback={(err, reset) => {
        return <div onClick={reset}>{`${err}`}</div>;
      }}
    >
      {resolved()}

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
