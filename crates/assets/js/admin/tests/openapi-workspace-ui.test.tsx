// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createSignal } from "solid-js";
import type { Tokens } from "trailbase";
import type { OpenApiDocument } from "../src/components/openapi/openapi";

type QueryOptions = {
  queryKey: readonly ["openapi"];
  queryFn: () => Promise<OpenApiDocument>;
};

type TestUser = { email?: string; username?: string };

const state = vi.hoisted(() => ({
  data: undefined as OpenApiDocument | undefined,
  error: undefined as Error | undefined,
  loading: true,
  fetching: false,
  user: undefined as TestUser | undefined,
  currentTokens: null as Tokens | null,
  queryOptions: undefined as QueryOptions | undefined,
  refetch: vi.fn(),
  fetch: vi.fn(),
  bumpQuery: undefined as (() => void) | undefined,
  bumpUser: undefined as (() => void) | undefined,
  listeners: [] as EventListener[],
  theme: "light" as "light" | "dark",
  mobile: false,
  bumpTheme: undefined as (() => void) | undefined,
  bumpMobile: undefined as (() => void) | undefined,
  added: [] as [string, EventListener][],
  removed: [] as [string, EventListener][],
}));
vi.mock("rapidoc", () => ({}));
vi.mock("@/lib/fetch", () => ({ adminFetch: state.fetch }));
vi.mock("@/lib/theme", () => ({
  createTheme: () => {
    const [v, set] = createSignal(state.theme);
    state.bumpTheme = () => set(state.theme);
    return v;
  },
}));
vi.mock("@/lib/signals", () => ({
  createIsMobile: () => {
    const [v, set] = createSignal(state.mobile);
    state.bumpMobile = () => set(state.mobile);
    return v;
  },
}));
vi.mock("@/lib/client", () => ({
  $user: {},
  $tokens: { get: () => state.currentTokens },
}));
vi.mock("@nanostores/solid", () => ({
  useStore: () => {
    const [value, setValue] = createSignal(state.user);
    state.bumpUser = () => setValue(state.user);
    return value;
  },
}));
vi.mock("@tanstack/solid-query", () => ({
  useQuery: (factory: () => QueryOptions) => {
    state.queryOptions = factory();
    const [tick, bump] = createSignal(0);
    state.bumpQuery = () => bump((v) => v + 1);
    return {
      get data() {
        tick();
        return state.data;
      },
      get error() {
        tick();
        return state.error;
      },
      get isLoading() {
        tick();
        return state.loading;
      },
      get isFetching() {
        tick();
        return state.fetching;
      },
      refetch: state.refetch,
    };
  },
}));
class TestRapiDoc extends HTMLElement {
  loadSpec = vi.fn();

  constructor() {
    super();
    const advancedInput = document.createElement("input");
    advancedInput.setAttribute("aria-label", "Advanced search");
    const input = document.createElement("input");
    input.setAttribute("aria-label", "Search endpoints");
    input.setAttribute("placeholder", "Filter");
    this.attachShadow({ mode: "open" }).append(advancedInput, input);
  }
  addEventListener(type: string, listener: EventListener) {
    if (type === "before-try") state.listeners.push(listener);
    state.added.push([type, listener]);
    return super.addEventListener(type, listener);
  }
  removeEventListener(type: string, listener: EventListener) {
    state.removed.push([type, listener]);
    return super.removeEventListener(type, listener);
  }
}
customElements.define("rapi-doc", TestRapiDoc);
import Page from "../src/components/openapi/OpenApiPage";

beforeEach(() => {
  const storage = () => ({
    setItem: vi.fn(),
    getItem: vi.fn(),
    removeItem: vi.fn(),
    clear: vi.fn(),
    key: vi.fn(),
    length: 0,
  });
  Object.defineProperty(window, "localStorage", {
    configurable: true,
    value: storage(),
  });
  Object.defineProperty(window, "sessionStorage", {
    configurable: true,
    value: storage(),
  });
  state.fetch.mockResolvedValue({ json: () => Promise.resolve({}) });
});
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  state.data = undefined;
  state.error = undefined;
  state.loading = true;
  state.fetching = false;
  state.user = undefined;
  state.currentTokens = null;
  state.queryOptions = undefined;
  state.listeners = [];
  state.added = [];
  state.removed = [];
  state.theme = "light";
  state.mobile = false;
});
const spec = (count = 1, version?: string) => ({
  info: { title: "Things", ...(version ? { version } : {}) },
  paths: {
    "/x": Object.fromEntries(
      Array.from({ length: count }, (_, i) => [i ? "post" : "get", {}]),
    ),
  },
});
const ready = (value = spec()) => {
  state.loading = false;
  state.data = value;
  render(() => <Page />);
  state.bumpQuery?.();
};

describe("OpenAPI Explorer workspace", () => {
  it("captures stable query and fetches JSON", async () => {
    render(() => <Page />);
    expect(state.queryOptions?.queryKey).toEqual(["openapi"]);
    const parsed = { info: { title: "Fetched" }, paths: {} };
    state.fetch.mockResolvedValueOnce({ json: () => Promise.resolve(parsed) });
    await expect(state.queryOptions?.queryFn()).resolves.toEqual(parsed);
    expect(state.fetch).toHaveBeenCalledWith("/openapi.json");
  });
  it("renders loading and success metadata with one initial load", () => {
    state.fetching = true;
    render(() => <Page />);
    expect(
      screen.getByRole("status", { name: /loading api specification/i }),
    ).toBeTruthy();
    expect(screen.queryByRole("status", { name: /refreshing/i })).toBeNull();
    expect(screen.getByRole("button", { name: /^refresh$/i })).toBeDisabled();
    state.loading = false;
    state.data = spec(1, "1");
    state.bumpQuery?.();
    expect(screen.getByText("OpenAPI Explorer")).toBeTruthy();
    expect(screen.getByText("Explore and try Things.")).toBeTruthy();
    expect(screen.getByText("v1")).toBeTruthy();
    expect(screen.getByText("1 operation")).toBeTruthy();
    expect(screen.getByText(/Server:/)).toHaveTextContent(
      "Server: http://localhost:4000",
    );
    const node = document.querySelector("rapi-doc") as TestRapiDoc;
    expect(node.loadSpec).toHaveBeenCalledTimes(1);
    const next = spec(1, "2");
    state.data = next;
    state.bumpQuery?.();
    expect(node.loadSpec).toHaveBeenCalledTimes(2);
    expect(node.loadSpec).toHaveBeenLastCalledWith(next);
    expect(node.getAttribute("server-url")).toBe("http://localhost:4000");
    expect(node.getAttribute("default-api-server")).toBe(
      "http://localhost:4000",
    );
  });
  it("configures focused RapiDoc and collapses tags without mutating source", () => {
    const value = { ...spec(), tags: [{ name: "Users" }] };
    ready(value);
    const node = document.querySelector("rapi-doc")!;
    expect(node).toHaveAttribute("render-style", "focused");
    expect(node).toHaveAttribute("schema-style", "table");
    expect(node).toHaveAttribute("show-side-nav", "true");
    expect(node).toHaveAttribute("allow-search", "true");
    expect(node).toHaveAttribute("primary-color", "#0073a8");
    expect(node.className).toContain("min-h-0");
    expect(node.parentElement?.className).toContain("overflow-hidden");
    expect((node as TestRapiDoc).loadSpec).toHaveBeenCalledWith(
      expect.objectContaining({
        tags: [{ name: "Users", "x-tag-expanded": false }],
      }),
    );
    expect(value.tags).toEqual([{ name: "Users" }]);
  });
  it("handles plural and absent versions", () => {
    ready(spec(2));
    expect(screen.getByText("2 operations")).toBeTruthy();
    cleanup();
    state.data = spec(2);
    state.bumpQuery?.();
    render(() => <Page />);
    expect(screen.queryByText(/^v/)).toBeNull();
  });
  it.each([
    [{ email: "e@example.com", username: "u" }, "e@example.com"],
    [{ username: "u" }, "u"],
    [undefined, "Admin session"],
  ])("shows identity fallback", (user, text) => {
    state.user = user;
    ready();
    expect(screen.getByText(`Identity: ${text}`)).toBeTruthy();
  });
  it("shows generic initial error and retry", () => {
    state.loading = false;
    state.error = new Error("secret backend detail");
    render(() => <Page />);
    expect(
      screen.getByText("Unable to load the API specification"),
    ).toBeTruthy();
    expect(screen.queryByText(/secret backend/)).toBeNull();
    fireEvent.click(screen.getByText("Retry"));
    expect(state.refetch).toHaveBeenCalled();
  });
  it("retains exact RapiDoc node and input on refresh error", () => {
    ready();
    const node = document.querySelector("rapi-doc");
    fireEvent.click(screen.getByText("Advanced authentication"));
    const input = screen.getByLabelText("Login tokens");
    const retained = btoa(
      JSON.stringify({ auth_token: "a", refresh_token: "r", csrf_token: "c" }),
    );
    fireEvent.input(input, { target: { value: retained } });
    state.error = new Error("x");
    state.fetching = true;
    state.bumpQuery?.();
    expect(document.querySelector("rapi-doc")).toBe(node);
    expect((input as HTMLInputElement).value).toBe(retained);
    expect(screen.getByText(/Unable to refresh/)).toBeTruthy();
    expect(screen.getByRole("button", { name: /refreshing/i })).toBeDisabled();
    expect(
      screen.getByRole("status", { name: /refreshing api specification/i }),
    ).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /retry/i }));
    expect(state.refetch).toHaveBeenCalled();
    state.error = undefined;
    state.fetching = false;
    state.bumpQuery?.();
    expect(document.querySelector("rapi-doc")).toBe(node);
    expect((input as HTMLInputElement).value).toBe(retained);
    expect(
      screen.getByRole("button", { name: /^refresh$/i }),
    ).not.toBeDisabled();
  });
  it("validates authentication and applies complete tokens", () => {
    const localSet = vi.spyOn(window.localStorage, "setItem");
    const sessionSet = vi.spyOn(window.sessionStorage, "setItem");
    state.currentTokens = {
      auth_token: "current-a",
      refresh_token: "current-r",
      csrf_token: "current-c",
    };
    ready();
    fireEvent.click(screen.getByText("Advanced authentication"));
    const input = screen.getByLabelText("Login tokens");
    const tokens = { auth_token: "a", refresh_token: "r", csrf_token: "c" };
    fireEvent.input(input, { target: { value: btoa(JSON.stringify(tokens)) } });
    expect(screen.getByText("Using impersonation tokens")).toBeTruthy();
    const request = new Request("http://localhost:4000/api");
    state.listeners[0](new CustomEvent("before-try", { detail: { request } }));
    expect(Object.fromEntries(request.headers)).toEqual({
      authorization: "Bearer a",
      "csrf-token": "c",
      "refresh-token": "r",
    });
    fireEvent.input(input, { target: { value: "bad-secret" } });
    const fallback = new Request("http://localhost:4000/api");
    expect(() =>
      state.listeners[0](
        new CustomEvent("before-try", { detail: { request: fallback } }),
      ),
    ).not.toThrow();
    expect(Object.fromEntries(fallback.headers)).toEqual({});
    expect(input).toHaveAttribute("aria-invalid", "true");
    expect(input.getAttribute("aria-describedby")).toContain(
      "openapi-tokens-error",
    );
    expect(screen.getByText("Invalid login tokens")).toBeTruthy();
    expect(screen.queryByText("bad-secret")).toBeNull();
    fireEvent.input(input, { target: { value: "" } });
    expect(screen.getByText("Using current admin session")).toBeTruthy();
    const cleared = new Request("http://localhost:4000/api");
    state.listeners[0](
      new CustomEvent("before-try", { detail: { request: cleared } }),
    );
    expect(cleared.headers.get("Authorization")).toBe("Bearer current-a");
    expect(localSet).not.toHaveBeenCalled();
    expect(sessionSet).not.toHaveBeenCalled();
    localSet.mockRestore();
    sessionSet.mockRestore();
  });
  it("keeps details closed, labels password tokens, and does not persist", () => {
    const localSet = vi.spyOn(localStorage, "setItem");
    const sessionSet = vi.spyOn(sessionStorage, "setItem");
    ready();
    const summary = screen.getByText("Advanced authentication");
    const details = summary.parentElement!;
    expect(details).not.toHaveAttribute("open");
    expect(details.closest("header")).toBeTruthy();
    expect(details).toHaveClass("relative");
    expect(details.querySelector("div")).toHaveClass("absolute");
    fireEvent.click(summary);
    const input = screen.getByLabelText("Login tokens");
    expect(input).toHaveAttribute("type", "password");
    expect(screen.getByText(/copied Accounts login tokens/)).toBeTruthy();
    expect(details).toHaveAttribute("open");
    expect(localSet).not.toHaveBeenCalled();
    expect(sessionSet).not.toHaveBeenCalled();
    localSet.mockRestore();
    sessionSet.mockRestore();
  });
  it("reacts to theme changes without reloading the spec", () => {
    ready();
    const node = document.querySelector("rapi-doc") as TestRapiDoc;
    const loads = node.loadSpec.mock.calls.length;
    state.theme = "dark";
    state.bumpTheme?.();
    expect(node.getAttribute("primary-color")).toBe("#38bdf8");
    expect(node.loadSpec).toHaveBeenCalledTimes(loads);
  });

  it("toggles mobile endpoint navigation without replacing RapiDoc", async () => {
    ready();
    const node = document.querySelector("rapi-doc") as TestRapiDoc;
    const loads = node.loadSpec.mock.calls.length;
    expect(
      screen.queryByRole("button", { name: "Browse endpoints" }),
    ).toBeNull();
    state.mobile = true;
    state.bumpMobile?.();
    const browse = screen.getByRole("button", { name: "Browse endpoints" });
    expect(browse).toHaveAttribute("aria-expanded", "false");
    expect(browse).toHaveAttribute("aria-controls", "openapi-endpoint-browser");
    fireEvent.click(browse);
    expect(browse).toHaveAttribute("aria-expanded", "true");
    await new Promise<void>((resolve) => queueMicrotask(() => resolve()));
    expect(node.shadowRoot?.activeElement).toBe(
      node.shadowRoot?.querySelector('input[placeholder="Filter"]'),
    );
    expect(node).toHaveClass("openapi-nav-open");
    expect(document.querySelector("rapi-doc")).toBe(node);
    expect(node.loadSpec).toHaveBeenCalledTimes(loads);
    const endpoint = document.createElement("button");
    endpoint.dataset.action = "navigate";
    node.shadowRoot?.append(endpoint);
    fireEvent.click(endpoint);
    expect(browse).toHaveAttribute("aria-expanded", "false");
    expect(document.activeElement).toBe(browse);
    fireEvent.click(browse);
    expect(browse).toHaveAttribute("aria-expanded", "true");
    fireEvent.click(browse);
    expect(browse).toHaveAttribute("aria-expanded", "false");
    expect(node).not.toHaveClass("openapi-nav-open");
    fireEvent.click(browse);
    await new Promise<void>((resolve) => queueMicrotask(() => resolve()));
    fireEvent.keyDown(document, { key: "Escape" });
    expect(browse).toHaveAttribute("aria-expanded", "false");
    expect(node).not.toHaveClass("openapi-nav-open");
    expect(document.activeElement).toBe(browse);
    state.mobile = false;
    state.bumpMobile?.();
    expect(
      screen.queryByRole("button", { name: "Browse endpoints" }),
    ).toBeNull();
    expect(node).not.toHaveClass("openapi-nav-open");
  });

  it("captures and removes both RapiDoc listeners", () => {
    const value = spec();
    ready(value);
    const node = document.querySelector("rapi-doc") as TestRapiDoc;
    const refs = state.added.filter(
      ([type]) => type === "before-try" || type === "spec-loaded",
    );
    const removeShadowListener = vi.spyOn(
      node.shadowRoot!,
      "removeEventListener",
    );
    cleanup();
    for (const [type, ref] of refs)
      expect(state.removed).toContainEqual([type, ref]);
    expect(removeShadowListener).toHaveBeenCalledWith(
      "click",
      expect.any(Function),
      true,
    );
    expect(removeShadowListener).toHaveBeenCalledWith(
      "keyup",
      expect.any(Function),
      true,
    );
    state.data = value;
    render(() => <Page />);
    state.bumpQuery?.();
    const next = document.querySelector("rapi-doc") as TestRapiDoc;
    expect(next).not.toBe(node);
    expect(next.loadSpec).toHaveBeenCalledTimes(1);
  });

  it("rejects credentials for cross-origin requests", () => {
    state.currentTokens = {
      auth_token: "a",
      refresh_token: "r",
      csrf_token: "c",
    };
    ready();
    const before = state.listeners[0];
    const cross = new Request("https://evil.example/api");
    before(new CustomEvent("before-try", { detail: { request: cross } }));
    expect(Object.fromEntries(cross.headers)).toEqual({});
    const local = new Request("http://localhost:4000/api");
    before(new CustomEvent("before-try", { detail: { request: local } }));
    expect(local.headers.get("authorization")).toBe("Bearer a");
  });

  it("safely invokes spec-loaded without a shadow root", () => {
    ready();
    const handler = state.added.find(([type]) => type === "spec-loaded")?.[1];
    expect(() => handler?.(new Event("spec-loaded"))).not.toThrow();
  });
});
