import { cleanup, render, screen } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { WasmComponent } from "@bindings/WasmComponent";

const queryState = vi.hoisted(() => ({
  data: "<html><body>dashboard</body></html>" as string | undefined,
  error: undefined as Error | undefined,
  isError: false,
  setData: undefined as ((value: string) => void) | undefined,
}));
const tokenState = vi.hoisted(() => ({
  subscribe: vi.fn(() => vi.fn()),
}));

vi.mock("@solidjs/router", () => ({
  A: (props: {
    href: string;
    title?: string;
    "aria-label"?: string;
    children: import("solid-js").JSX.Element;
  }) => (
    <a href={props.href} title={props.title} aria-label={props["aria-label"]}>
      {props.children}
    </a>
  ),
}));
vi.mock("@tanstack/solid-query", () => ({
  useQuery: () => {
    const [version, setVersion] = createSignal(0);
    queryState.setData = (value) => {
      queryState.data = value;
      setVersion((current) => current + 1);
    };
    return {
      get data() {
        version();
        return queryState.data;
      },
      get error() {
        return queryState.error;
      },
      get isError() {
        return queryState.isError;
      },
    };
  },
}));
vi.mock("@/lib/client", () => ({
  client: { headers: vi.fn(() => ({})) },
  hostAddress: () => "http://localhost",
  $tokens: tokenState,
}));
vi.mock("@/lib/theme", () => ({
  currentTheme: () => "light",
}));

import { WasmComponentDetails } from "@/components/wasm/WasmComponentDetails";

const component = {
  name: "trailbase/auth_ui",
  path: "components/auth_ui.wasm",
  loaded: true,
  installed: true,
  admin_ui_path: "/dashboard",
} satisfies WasmComponent;

afterEach(cleanup);
beforeEach(() => {
  queryState.data = "<html><body>dashboard</body></html>";
  queryState.error = undefined;
  queryState.isError = false;
  queryState.setData = undefined;
  tokenState.subscribe.mockReset().mockImplementation(() => vi.fn());
});

describe("WASM component details", () => {
  it("gives the Back link a matching accessible name and title", () => {
    render(() => (
      <WasmComponentDetails component={component} sandboxed={false} />
    ));

    const back = screen.getByRole("link", {
      name: "Back to the list of WASM components",
    });
    expect(back).toHaveAttribute(
      "title",
      "Back to the list of WASM components",
    );
    expect(back).toHaveAttribute(
      "aria-label",
      "Back to the list of WASM components",
    );
  });

  it("removes old iframe load handlers and token subscriptions on reruns", () => {
    render(() => (
      <WasmComponentDetails component={component} sandboxed={true} />
    ));
    const iframe = screen.getByTitle("WASM component preview");
    const removeEventListener = vi.spyOn(iframe, "removeEventListener");

    iframe.dispatchEvent(new Event("load"));
    expect(tokenState.subscribe).toHaveBeenCalledOnce();
    const unsubscribe = tokenState.subscribe.mock.results[0]?.value;

    queryState.setData!("<html><body>updated dashboard</body></html>");

    expect(removeEventListener).toHaveBeenCalledWith(
      "load",
      expect.any(Function),
    );
    expect(unsubscribe).toHaveBeenCalledOnce();
  });

  it("shows stable safe copy for dashboard query errors", () => {
    queryState.data = undefined;
    queryState.error = new Error("backend secret");
    queryState.isError = true;
    render(() => (
      <WasmComponentDetails component={component} sandboxed={true} />
    ));

    expect(
      screen.getByText(/unable to load the component dashboard/i),
    ).toBeInTheDocument();
    expect(screen.queryByText("backend secret")).not.toBeInTheDocument();
  });
});
