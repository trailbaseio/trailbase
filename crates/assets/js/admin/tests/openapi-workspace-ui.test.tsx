// @vitest-environment jsdom
import { cleanup, render, screen } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { createSignal } from "solid-js";

const state = vi.hoisted(() => {
  const [data, setData] = createSignal<any>();
  const [error, setError] = createSignal<any>();
  const [loading, setLoading] = createSignal(true);
  const [fetching, setFetching] = createSignal(false);
  return {
    data,
    setData,
    error,
    setError,
    loading,
    setLoading,
    fetching,
    setFetching,
    refetch: vi.fn(),
    fetch: vi.fn(),
    user: vi.fn(() => undefined),
    tokens: { get: vi.fn(() => null) },
  };
});
vi.mock("rapidoc", () => ({}));
vi.mock("@/lib/fetch", () => ({ adminFetch: state.fetch }));
vi.mock("@/lib/theme", () => ({ createTheme: () => () => "light" }));
vi.mock("@/lib/client", () => ({ $user: {}, $tokens: state.tokens }));
vi.mock("@nanostores/solid", () => ({ useStore: state.user }));
vi.mock("@tanstack/solid-query", () => ({
  useQuery: () => ({
    get data() {
      return state.data();
    },
    get error() {
      return state.error();
    },
    get isLoading() {
      return state.loading();
    },
    get isFetching() {
      return state.fetching();
    },
    refetch: state.refetch,
  }),
}));

class TestRapiDoc extends HTMLElement {
  loadSpec = vi.fn();
}
customElements.define("rapi-doc", TestRapiDoc);
import Page from "../src/components/openapi/OpenApiPage";

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  state.setData(undefined);
  state.setError(undefined);
  state.setLoading(true);
  state.setFetching(false);
});

describe("OpenAPI Explorer workspace", () => {
  it("renders loading status and query-owned success metadata", async () => {
    render(() => <Page />);
    expect(screen.getByRole("status")).toBeTruthy();
    state.setLoading(false);
    state.setData({
      info: { title: "Things", version: "1" },
      paths: { "/x": { get: {} } },
    });
    expect(await screen.findByText("OpenAPI Explorer")).toBeTruthy();
    expect(screen.getByText("Explore and try Things.")).toBeTruthy();
    expect(screen.getByText("v1")).toBeTruthy();
    expect(screen.getByText("1 operation")).toBeTruthy();
    expect(screen.getByText("http://localhost:4000")).toBeTruthy();
  });

  it("keeps authentication details closed and validates tokens without persistence", async () => {
    state.setLoading(false);
    state.setData({ info: {}, paths: {} });
    render(() => <Page />);
    const details = await screen.findByText("Advanced authentication");
    expect(details.parentElement?.hasAttribute("open")).toBe(false);
    const localSet = vi.spyOn(Storage.prototype, "setItem");
    expect(localSet).not.toHaveBeenCalled();
  });
});
