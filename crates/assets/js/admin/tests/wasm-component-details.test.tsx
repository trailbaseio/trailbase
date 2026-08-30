import { cleanup, render, screen } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { WasmComponent } from "@bindings/WasmComponent";

const queryState = vi.hoisted(() => ({
  data: "<html><body>dashboard</body></html>" as string | undefined,
  error: undefined as Error | undefined,
  isError: false,
  isLoading: false,
  refetch: vi.fn(),
  queryFn: undefined as
    | ((context: { queryKey: unknown[] }) => Promise<string | undefined>)
    | undefined,
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
  useQuery: (
    options: () => {
      queryFn: (context: {
        queryKey: unknown[];
      }) => Promise<string | undefined>;
    },
  ) => {
    queryState.queryFn = options().queryFn;
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
      get isLoading() {
        return queryState.isLoading;
      },
      refetch: queryState.refetch,
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

import {
  injectCspMeta,
  WasmComponentDetails,
} from "@/components/wasm/WasmComponentDetails";

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
  queryState.isLoading = false;
  queryState.refetch.mockReset();
  queryState.queryFn = undefined;
  queryState.setData = undefined;
  tokenState.subscribe.mockReset().mockImplementation(() => vi.fn());
});

describe("CSP injection", () => {
  it.each([
    ["normal head", "<html><head><title>x</title></head><body>x</body></html>"],
    ["html without head", "<html><body>x</body></html>"],
    ["fragment", "<div>x</div>"],
  ])("adds a CSP meta to %s without duplicate heads", (_name, body) => {
    const result = injectCspMeta(body, "default-src 'self' https://example.test");
    expect(result.match(/<head\b/gi)).toHaveLength(1);
    expect(result).toContain('http-equiv="Content-Security-Policy"');
  });
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

  it("rejects non-OK dashboard responses without exposing response content", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(new Response("backend secret", { status: 500 }));
    vi.stubGlobal("fetch", fetchMock);
    render(() => (
      <WasmComponentDetails component={component} sandboxed={true} />
    ));

    await expect(queryState.queryFn?.({ queryKey: [] })).rejects.toThrow(
      "dashboard request failed",
    );
    expect(fetchMock).toHaveBeenCalledOnce();
    vi.unstubAllGlobals();
  });

  it("shows stable safe copy and Retry for dashboard query errors", () => {
    queryState.data = undefined;
    queryState.error = new Error("backend secret");
    queryState.isError = true;
    render(() => (
      <WasmComponentDetails component={component} sandboxed={true} />
    ));

    expect(
      screen.getByText(/unable to load the component dashboard/i),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /retry/i })).toBeInTheDocument();
    expect(screen.queryByText("backend secret")).not.toBeInTheDocument();

    screen.getByRole("button", { name: /retry/i }).click();
    expect(queryState.refetch).toHaveBeenCalledOnce();
  });

  it("keeps the iframe mounted and shows loading", () => {
    queryState.data = undefined;
    queryState.isLoading = true;
    render(() => (
      <WasmComponentDetails component={component} sandboxed={true} />
    ));

    expect(screen.getByTitle("WASM component preview")).toBeInTheDocument();
    expect(
      screen.getByText(/loading component dashboard/i),
    ).toBeInTheDocument();
  });

  it("controls the sandbox toggle and switches iframe modes", () => {
    render(() => (
      <WasmComponentDetails component={component} sandboxed={true} />
    ));

    const toggle = screen.getByRole("switch", { name: "Sandboxed" });
    expect(toggle).toHaveAttribute("aria-checked", "true");
    toggle.click();
    expect(toggle).toHaveAttribute("aria-checked", "false");
    expect(screen.getByTitle("WASM component dashboard")).toHaveAttribute(
      "src",
      "http://localhost:4000/dashboard",
    );
    expect(
      screen.queryByTitle("WASM component preview"),
    ).not.toBeInTheDocument();
  });

  it("shows display and internal names with the version", () => {
    render(() => (
      <WasmComponentDetails
        component={{ ...component, display_name: "Auth UI", version: "1.2.3" }}
        sandboxed={true}
      />
    ));

    expect(
      screen.getByRole("heading", { name: "Auth UI" }),
    ).toBeInTheDocument();
    expect(
      screen.getByText("Internal name: trailbase/auth_ui"),
    ).toBeInTheDocument();
    expect(screen.getByText(/1\.2\.3/)).toBeInTheDocument();
  });

  it("shows a return action when the component has no dashboard", () => {
    render(() => (
      <WasmComponentDetails
        component={{ ...component, admin_ui_path: undefined }}
        sandboxed={true}
      />
    ));

    expect(
      screen.getByText(/trailbase\/auth_ui.*no dashboard/i),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("link", { name: "Back to the list of WASM components" }),
    ).toHaveAttribute("href", "/wasm");
  });

  it.each([
    "https://evil.example/",
    "//evil.example/",
    "///evil.example/",
    "/\\\\evil.example",
  ])(
    "rejects unsafe dashboard path %s before fetch or navigation",
    (admin_ui_path) => {
      const fetchMock = vi.fn();
      vi.stubGlobal("fetch", fetchMock);

      render(() => (
        <WasmComponentDetails
          component={{ ...component, admin_ui_path }}
          sandboxed={true}
        />
      ));

      expect(screen.getByText(/rejected for safety/i)).toBeInTheDocument();
      expect(
        screen.queryByTitle("WASM component preview"),
      ).not.toBeInTheDocument();
      expect(
        screen.queryByTitle("WASM component dashboard"),
      ).not.toBeInTheDocument();
      expect(fetchMock).not.toHaveBeenCalled();
      vi.unstubAllGlobals();
    },
  );
});
