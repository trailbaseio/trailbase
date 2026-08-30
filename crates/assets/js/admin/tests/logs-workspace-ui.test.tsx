import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@solidjs/testing-library";
import * as Solid from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const state = vi.hoisted(() => ({
  params: {} as Record<string, string | undefined>,
  setParams: vi.fn(),
  fetchLogs: vi.fn(),
  fetchStats: vi.fn(),
  refetch: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("@solidjs/router", () => ({
  useSearchParams: () => {
    const [version, setVersion] = Solid.createSignal(0);
    const params = new Proxy(state.params, {
      get: (target, key: string) => {
        version();
        return target[key];
      },
    });
    state.setParams.mockImplementation((next) => {
      Object.assign(state.params, next);
      setVersion((n) => n + 1);
    });
    return [params, state.setParams];
  },
}));
vi.mock("@tanstack/solid-query", () => ({
  useQuery: (factory: () => { queryFn: () => Promise<unknown> }) => {
    const [version, setVersion] = Solid.createSignal(0);
    Solid.createEffect(() => {
      const options = factory();
      void options.queryFn().then(() => setVersion((n) => n + 1));
    });
    return {
      get data() {
        version();
        return undefined;
      },
      get error() {
        return undefined;
      },
      get isLoading() {
        return false;
      },
      get isFetching() {
        return false;
      },
      get isRefetching() {
        return false;
      },
      refetch: state.refetch,
    };
  },
}));
vi.mock("@/lib/api/logs", () => ({
  fetchLogs: state.fetchLogs,
  fetchStats: state.fetchStats,
}));
vi.mock("@/components/logs/LogsInsights", () => ({
  LogsInsights: () => <div>Insights</div>,
}));
import LogsPage from "@/components/logs/LogsPage";

afterEach(cleanup);
beforeEach(() => {
  vi.clearAllMocks();
  state.params = {};
  state.fetchLogs.mockResolvedValue({ entries: [], total_row_count: 0 });
  state.fetchStats.mockResolvedValue({ rates: [], country_codes: null });
});

describe("LogsPage request workspace", () => {
  it("uses explicit list and stats query keys and presents the dense workspace", async () => {
    render(() => <LogsPage />);
    await waitFor(() =>
      expect(state.fetchLogs).toHaveBeenCalledWith(
        20,
        0,
        undefined,
        undefined,
        undefined,
      ),
    );
    expect(screen.getByRole("heading", { name: "Logs" })).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Refresh logs" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Filter syntax")).toBeInTheDocument();
    for (const heading of [
      "Time",
      "Status",
      "Method",
      "Request",
      "Latency",
      "Client",
      "User",
    ])
      expect(screen.getByText(heading)).toBeInTheDocument();
  });

  it("applies and clears filters without resetting on query errors", async () => {
    render(() => <LogsPage />);
    const input = screen.getByRole("textbox", { name: "Filter rows" });
    await fireEvent.input(input, { target: { value: "status >= 500" } });
    await fireEvent.click(screen.getByRole("button", { name: "Apply filter" }));
    expect(state.setParams).toHaveBeenCalledWith(
      expect.objectContaining({
        filter: "status >= 500",
        pageIndex: undefined,
        pageSize: undefined,
      }),
    );
  });
});
