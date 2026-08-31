// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createSignal } from "solid-js";

const state = vi.hoisted(() => ({
  data: undefined as any,
  error: undefined as any,
  loading: true,
  fetching: false,
  user: undefined as any,
  currentTokens: null as any,
  queryOptions: undefined as any,
  refetch: vi.fn(),
  fetch: vi.fn(),
  bumpQuery: undefined as (() => void) | undefined,
  bumpUser: undefined as (() => void) | undefined,
  listeners: [] as EventListener[],
}));
vi.mock("rapidoc", () => ({}));
vi.mock("@/lib/fetch", () => ({ adminFetch: state.fetch }));
vi.mock("@/lib/theme", () => ({ createTheme: () => () => "light" }));
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
  useQuery: (factory: () => any) => {
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
  addEventListener(type: string, listener: EventListener) {
    if (type === "before-try") state.listeners.push(listener);
    return super.addEventListener(type, listener);
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
    render(() => <Page />);
    expect(
      screen.getByRole("status", { name: /loading api specification/i }),
    ).toBeTruthy();
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
});
