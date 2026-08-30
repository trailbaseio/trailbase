import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@solidjs/testing-library";
import * as Solid from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { ListLogsResponse } from "@bindings/ListLogsResponse";
import type { LogJson } from "@bindings/LogJson";
import type { StatsResponse } from "@bindings/StatsResponse";

const log: LogJson = {
  id: 1n,
  created: 1_700_000_000,
  status: 200,
  method: "GET",
  url: "/api/items",
  latency_ms: 12.5,
  client_ip: "192.0.2.1",
  client_geoip_cc: "FR",
  client_geoip_city: { name: "Paris", country_code: "FR" },
  referer: "",
  user_agent: "TestAgent/1.0",
  user_id: null,
};

const secondLog: LogJson = {
  ...log,
  id: 2n,
  created: 1_700_000_100,
  status: 503,
  method: "POST",
  url: "/api/items/2",
  latency_ms: 1_250,
  client_ip: "192.0.2.2",
  client_geoip_cc: null,
  client_geoip_city: null,
  user_id: "user-2",
};

type QueryOptions = {
  queryKey: readonly unknown[];
  queryFn: () => Promise<unknown>;
};

type QueryKind = "list" | "stats";

const state = vi.hoisted(() => ({
  params: {} as Record<string, string | undefined>,
  setParams: vi.fn(),
  bumpParams: undefined as (() => void) | undefined,
  list: {
    data: undefined as ListLogsResponse | undefined,
    error: undefined as Error | undefined,
    isLoading: false,
    isFetching: false,
    isRefetching: false,
    refetch: vi.fn(),
    bump: undefined as (() => void) | undefined,
  },
  stats: {
    data: undefined as StatsResponse | undefined,
    error: undefined as Error | undefined,
    isLoading: false,
    isFetching: false,
    isRefetching: false,
    refetch: vi.fn(),
    bump: undefined as (() => void) | undefined,
  },
  listOptions: [] as QueryOptions[],
  statsOptions: [] as QueryOptions[],
  fetchLogs: vi.fn(),
  fetchStats: vi.fn(),
  insightsProps: undefined as
    | {
        error: Error | undefined;
        loading: boolean;
        onRetry: () => void;
      }
    | undefined,
}));

vi.mock("@solidjs/router", () => ({
  useSearchParams: () => {
    const [version, setVersion] = Solid.createSignal(0);
    state.bumpParams = () => setVersion((value) => value + 1);
    const params = new Proxy(state.params, {
      get: (_target, key: string) => {
        version();
        return state.params[key];
      },
    });
    state.setParams.mockImplementation(
      (next: Record<string, string | undefined>) => {
        Object.assign(state.params, next);
        state.bumpParams?.();
      },
    );
    return [params, state.setParams];
  },
}));

vi.mock("@tanstack/solid-query", () => ({
  useQuery: (factory: () => QueryOptions) => {
    const initialOptions = factory();
    const kind: QueryKind =
      initialOptions.queryKey[0] === "logs" ? "list" : "stats";
    const box = state[kind];
    const options = kind === "list" ? state.listOptions : state.statsOptions;
    const [version, setVersion] = Solid.createSignal(0);
    let latestOptions = initialOptions;
    let latestKey = JSON.stringify(initialOptions.queryKey);
    options.push(initialOptions);

    const execute = (queryOptions: QueryOptions) =>
      queryOptions.queryFn().then(
        (data) => {
          box.data = data as typeof box.data;
          box.error = undefined;
          setVersion((value) => value + 1);
          return data;
        },
        (error: unknown) => {
          box.error = error instanceof Error ? error : new Error(String(error));
          setVersion((value) => value + 1);
          throw error;
        },
      );

    Solid.createEffect(() => {
      const nextOptions = factory();
      const nextKey = JSON.stringify(nextOptions.queryKey);
      latestOptions = nextOptions;
      options.push(nextOptions);
      if (nextKey !== latestKey) {
        latestKey = nextKey;
        void execute(nextOptions).catch(() => undefined);
      }
    });

    const refetch = vi.fn(() => execute(latestOptions));
    box.refetch = refetch;
    box.bump = () => setVersion((value) => value + 1);
    void execute(initialOptions).catch(() => undefined);

    return {
      get data() {
        version();
        return box.data;
      },
      get error() {
        version();
        return box.error;
      },
      get isLoading() {
        version();
        return box.isLoading;
      },
      get isFetching() {
        version();
        return box.isFetching;
      },
      get isRefetching() {
        version();
        return box.isRefetching;
      },
      refetch,
    };
  },
}));

vi.mock("@/lib/api/logs", () => ({
  fetchLogs: state.fetchLogs,
  fetchStats: state.fetchStats,
}));

vi.mock("@/components/logs/LogsInsights", () => ({
  LogsInsights: (props: {
    error: Error | undefined;
    loading: boolean;
    onRetry: () => void;
  }) => {
    // The mock captures props so tests can verify query isolation.
    // eslint-disable-next-line solid/reactivity
    state.insightsProps = props;
    return (
      <div data-testid="insights">
        <span>{props.loading ? "Insights loading" : "Insights"}</span>
        {props.error && <span>Insights unavailable</span>}
        <button type="button" onClick={() => props.onRetry()}>
          Retry insights
        </button>
      </div>
    );
  },
}));

import LogsPage from "@/components/logs/LogsPage";

const response = (
  entries: LogJson[] = [log],
  totalRowCount: bigint = BigInt(entries.length),
  cursor: string | null = null,
): ListLogsResponse => ({
  entries,
  total_row_count: totalRowCount,
  cursor,
});

function resetState() {
  state.params = {};
  state.bumpParams = undefined;
  state.list.data = undefined;
  state.list.error = undefined;
  state.list.isLoading = false;
  state.list.isFetching = false;
  state.list.isRefetching = false;
  state.list.bump = undefined;
  state.stats.data = undefined;
  state.stats.error = undefined;
  state.stats.isLoading = false;
  state.stats.isFetching = false;
  state.stats.isRefetching = false;
  state.stats.bump = undefined;
  state.listOptions = [];
  state.statsOptions = [];
  state.insightsProps = undefined;
  state.fetchLogs.mockReset().mockResolvedValue(response());
  state.fetchStats
    .mockReset()
    .mockResolvedValue({ rates: [], country_codes: null });
  state.setParams.mockReset();
  state.list.refetch.mockReset();
  state.stats.refetch.mockReset();
}

async function setup() {
  render(() => <LogsPage />);
  await waitFor(() => expect(state.fetchLogs).toHaveBeenCalled());
}

beforeEach(resetState);
afterEach(cleanup);

describe("LogsPage query state", () => {
  it("uses primitive query keys and initial fetch arguments", async () => {
    await setup();

    expect(state.listOptions[0].queryKey).toEqual(["logs", 20, 0, "", ""]);
    expect(state.statsOptions[0].queryKey).toEqual(["log-stats", ""]);
    expect(state.fetchLogs).toHaveBeenCalledWith(
      20,
      0,
      undefined,
      undefined,
      undefined,
    );
    expect(state.fetchStats).toHaveBeenCalledWith(undefined);
  });

  it("applies and clears filters, resubmits both queries, and preserves state on errors", async () => {
    state.params = { pageIndex: "2", pageSize: "50" };
    await setup();
    const input = screen.getByRole("textbox", { name: "Filter rows" });

    await fireEvent.input(input, { target: { value: "status >= 500" } });
    await fireEvent.click(screen.getByRole("button", { name: "Apply filter" }));
    expect(state.params).toMatchObject({
      filter: "status >= 500",
      pageIndex: undefined,
      pageSize: undefined,
    });

    const listRefetches = state.list.refetch.mock.calls.length;
    const statsRefetches = state.stats.refetch.mock.calls.length;
    await fireEvent.input(input, { target: { value: "status >= 500" } });
    await fireEvent.click(screen.getByRole("button", { name: "Apply filter" }));
    expect(state.list.refetch.mock.calls.length).toBe(listRefetches + 1);
    expect(state.stats.refetch.mock.calls.length).toBe(statsRefetches + 1);

    await fireEvent.click(screen.getByRole("button", { name: "Clear filter" }));
    expect(state.params).toMatchObject({
      filter: undefined,
      pageIndex: undefined,
      pageSize: undefined,
    });

    state.setParams.mockClear();
    state.fetchLogs.mockRejectedValueOnce(new Error("backend boom"));
    state.fetchStats.mockRejectedValueOnce(new Error("stats boom"));
    await expect(state.list.refetch()).rejects.toThrow("backend boom");
    await expect(state.stats.refetch()).rejects.toThrow("stats boom");
    expect(state.setParams).not.toHaveBeenCalled();
  });

  it("sorts locally, resets pagination, never writes order, and preserves filters when paginating", async () => {
    state.params = { filter: "status >= 400", pageIndex: "1", pageSize: "50" };
    await setup();
    const status = screen.getByRole("columnheader", { name: "Status" });
    await fireEvent.click(status);

    expect(state.params).toMatchObject({
      filter: "status >= 400",
      pageIndex: undefined,
      pageSize: undefined,
    });
    await waitFor(() =>
      expect(state.listOptions.at(-1)?.queryKey).toEqual([
        "logs",
        20,
        0,
        "status >= 400",
        "-status",
      ]),
    );
    expect(state.fetchLogs.mock.calls.at(-1)).toEqual([
      20,
      0,
      "status >= 400",
      undefined,
      "-status",
    ]);

    await fireEvent.click(
      screen.getByRole("button", { name: "Go to next page" }),
    );
    expect(state.params).toMatchObject({ filter: "status >= 400" });
    expect(state.params.order).toBeUndefined();
  });

  it("passes cursors to the next page and resets them when the query changes", async () => {
    state.fetchLogs
      .mockResolvedValueOnce(response([log], 2n, "cursor-0"))
      .mockResolvedValue(response([secondLog], 2n, "cursor-1"));
    await setup();
    expect(state.fetchLogs).toHaveBeenCalledWith(
      20,
      0,
      undefined,
      undefined,
      undefined,
    );

    state.setParams({ pageIndex: "1" });
    await waitFor(() =>
      expect(state.fetchLogs).toHaveBeenCalledWith(
        20,
        1,
        undefined,
        "cursor-0",
        undefined,
      ),
    );

    state.setParams({
      filter: "status >= 400",
      pageIndex: undefined,
      pageSize: undefined,
    });
    await waitFor(() =>
      expect(state.fetchLogs).toHaveBeenCalledWith(
        20,
        0,
        "status >= 400",
        undefined,
        undefined,
      ),
    );
  });

  it("refreshes both queries while retaining rows and showing progress", async () => {
    await setup();
    expect(screen.getByText("/api/items")).toBeInTheDocument();
    state.list.isFetching = true;
    state.list.isRefetching = true;
    state.stats.isFetching = true;
    state.stats.isRefetching = true;
    state.list.bump?.();
    state.stats.bump?.();

    const refresh = screen.getByRole("button", { name: "Refresh logs" });
    expect(refresh).toBeDisabled();
    expect(refresh).toHaveTextContent("Refreshing");
    state.list.isFetching = false;
    state.list.isRefetching = false;
    state.stats.isFetching = false;
    state.stats.isRefetching = false;
    state.list.bump?.();
    state.stats.bump?.();
    await fireEvent.click(screen.getByRole("button", { name: "Refresh logs" }));
    expect(state.list.refetch).toHaveBeenCalledTimes(1);
    expect(state.stats.refetch).toHaveBeenCalledTimes(1);
    expect(screen.getByText("/api/items")).toBeInTheDocument();
  });
});

describe("LogsPage presentation and states", () => {
  it("shows independent list and insights errors without raw details", async () => {
    state.fetchLogs.mockRejectedValue(new Error("backend boom"));
    await setup();
    await waitFor(() =>
      expect(screen.getByText("Unable to load requests")).toBeInTheDocument(),
    );
    expect(screen.queryByText("backend boom")).not.toBeInTheDocument();
    expect(
      screen.getByRole("textbox", { name: "Filter rows" }),
    ).toBeInTheDocument();
    expect(screen.getByTestId("insights")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();

    cleanup();
    resetState();
    state.fetchStats.mockRejectedValue(new Error("stats boom"));
    await setup();
    await waitFor(() =>
      expect(screen.getByText("Insights unavailable")).toBeInTheDocument(),
    );
    expect(screen.getByText("/api/items")).toBeInTheDocument();
    expect(state.insightsProps?.error?.message).toBe("stats boom");
    const retries = state.stats.refetch.mock.calls.length;
    await fireEvent.click(
      screen.getByRole("button", { name: "Retry insights" }),
    );
    expect(state.stats.refetch.mock.calls.length).toBe(retries + 1);
  });

  it("shows accurate request counts when data is available and omits them while unavailable", async () => {
    state.list.data = undefined;
    state.fetchLogs.mockReturnValue(
      new Promise<ListLogsResponse>(() => undefined),
    );
    await setup();
    expect(screen.queryByText(/requests$/)).not.toBeInTheDocument();

    cleanup();
    resetState();
    state.params = { filter: "status >= 400" };
    state.fetchLogs.mockResolvedValue(response([secondLog], 1n));
    await setup();
    await waitFor(() =>
      expect(screen.getByText("1 requests")).toBeInTheDocument(),
    );
    expect(screen.queryByText("2 requests")).not.toBeInTheDocument();
  });

  it("renders the dense seven-column row and contained overflow", async () => {
    state.fetchLogs.mockResolvedValue(response([log], 3n));
    await setup();
    expect(
      screen.getAllByRole("columnheader").map((header) => header.textContent),
    ).toEqual([
      "Time",
      "Status",
      "Method",
      "Request",
      "Latency",
      "Client",
      "User",
    ]);
    const time = document.querySelector("time");
    expect(time).toHaveAttribute("datetime", "2023-11-14T22:13:20.000Z");
    expect(time).toHaveAttribute("title", "2023-11-14T22:13:20.000Z");
    expect(screen.getByText("200")).toHaveClass("bg-success");
    expect(screen.getByText("GET")).toHaveClass("border");
    expect(screen.getByText("/api/items")).toBeInTheDocument();
    expect(screen.getByText("12.5 ms")).toBeInTheDocument();
    expect(screen.getByText("Paris, FR")).toBeInTheDocument();
    expect(screen.getByText("192.0.2.1")).toBeInTheDocument();
    expect(screen.getByText("—")).toBeInTheDocument();
    expect(screen.getByRole("table").closest(".overflow-x-auto")).toHaveClass(
      "min-w-0",
    );
    expect(screen.getByRole("row", { name: /GET.*api\/items/ })).toHaveClass(
      "h-9",
    );
  });

  it("keeps controls during loading and distinguishes empty states", async () => {
    state.list.isLoading = true;
    state.stats.isLoading = true;
    state.fetchLogs.mockReturnValue(
      new Promise<ListLogsResponse>(() => undefined),
    );
    state.fetchStats.mockReturnValue(
      new Promise<StatsResponse>(() => undefined),
    );
    render(() => <LogsPage />);
    expect(screen.getByRole("heading", { name: "Logs" })).toBeInTheDocument();
    expect(
      screen.getByRole("textbox", { name: "Filter rows" }),
    ).toBeInTheDocument();
    expect(screen.getByTestId("insights")).toHaveTextContent(
      "Insights loading",
    );
    expect(screen.getAllByRole("row").length).toBeGreaterThan(1);

    cleanup();
    resetState();
    state.fetchLogs.mockResolvedValue(response([], 0n));
    await setup();
    await waitFor(() =>
      expect(
        screen.getByText("No requests have been recorded"),
      ).toBeInTheDocument(),
    );

    cleanup();
    resetState();
    state.params = { filter: "status >= 400" };
    state.fetchLogs.mockResolvedValue(response([], 0n));
    await setup();
    await waitFor(() =>
      expect(screen.getByText("No requests match")).toBeInTheDocument(),
    );
    const clearButtons = screen.getAllByRole("button", {
      name: "Clear filter",
    });
    await fireEvent.click(clearButtons.at(-1)!);
    expect(state.params).toMatchObject({
      filter: undefined,
      pageIndex: undefined,
      pageSize: undefined,
    });
  });

  it("documents valid filter syntax without inventing contains", async () => {
    await setup();
    const syntax = screen.getByText("Filter syntax").parentElement;
    expect(syntax).toHaveTextContent(/status/);
    expect(syntax).toHaveTextContent(/method/);
    expect(syntax).toHaveTextContent(/latency/);
    expect(syntax).toHaveTextContent(/url ~/);
    expect(syntax).toHaveTextContent(/client_ip/);
    expect(syntax).toHaveTextContent(/user_id/);
    expect(syntax).not.toHaveTextContent("contains");
  });
});

describe("LogsPage request inspection", () => {
  it("opens the real portaled sheet and restores focus after Escape", async () => {
    await setup();
    const row = screen.getAllByRole("row")[1] as HTMLTableRowElement;
    await fireEvent.click(row);
    await waitFor(() =>
      expect(
        screen.getByRole("heading", { name: "GET /api/items" }),
      ).toBeInTheDocument(),
    );
    expect(document.body).toHaveTextContent("192.0.2.1");
    expect(document.body).toHaveTextContent("TestAgent/1.0");
    await fireEvent.keyDown(document, { key: "Escape" });
    await waitFor(() => expect(document.activeElement).toBe(row));
  });

  it("opens from Enter and Space keyboard activation", async () => {
    await setup();
    const row = screen.getAllByRole("row")[1] as HTMLTableRowElement;
    await fireEvent.keyDown(row, { key: "Enter" });
    await waitFor(() =>
      expect(document.body).toHaveTextContent("Request details"),
    );
    await fireEvent.click(screen.getByRole("button", { name: "Dismiss" }));
    await waitFor(() => expect(document.activeElement).toBe(row));

    await fireEvent.keyDown(row, { key: " " });
    await waitFor(() =>
      expect(document.body).toHaveTextContent("Request details"),
    );
    expect(
      screen.getByRole("heading", { name: "GET /api/items" }),
    ).toBeInTheDocument();
  });
});
