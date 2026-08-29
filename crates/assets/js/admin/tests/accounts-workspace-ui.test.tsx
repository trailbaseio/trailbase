import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@solidjs/testing-library";
import * as Solid from "solid-js";
import { createEffect } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const pageState = vi.hoisted(() => ({
  params: {
    search: "ada",
    filter: "admin = TRUE",
    advanced: "false",
    pageSize: "25",
    pageIndex: "2",
  } as Record<string, string | undefined>,
  setSearchParams: vi.fn(),
  fetchUsers: vi.fn(),
  invalidateQueries: vi.fn(),
  queryError: false,
  queryLoading: false,
  queryData: { users: [] as Array<{ id: string; email?: string }> },
  setDataVersion: undefined as (() => void) | undefined,
  showEmptyState: false,
}));

vi.mock("@solidjs/router", () => ({
  useSearchParams: () => {
    const [version, setVersion] = Solid.createSignal(0);
    const proxy = new Proxy(pageState.params, {
      get: (_target, key: string) => {
        version();
        return pageState.params[key];
      },
    });
    pageState.setSearchParams.mockImplementation((next) => {
      Object.assign(pageState.params, next);
      setVersion((value) => value + 1);
    });
    return [proxy, pageState.setSearchParams];
  },
}));
vi.mock("@tanstack/solid-query", () => ({
  useQuery: (options: () => { queryFn: () => Promise<unknown> }) => {
    const [dataVersion, setDataVersion] = Solid.createSignal(0);
    pageState.setDataVersion = () => setDataVersion((value) => value + 1);
    createEffect(() => void options().queryFn());
    return {
      get isLoading() {
        return pageState.queryLoading;
      },
      get isError() {
        return pageState.queryError;
      },
      get data() {
        dataVersion();
        return pageState.queryData;
      },
    };
  },
  useQueryClient: () => ({ invalidateQueries: pageState.invalidateQueries }),
}));
vi.mock("@/lib/api/user", () => ({
  fetchUsers: pageState.fetchUsers,
  deleteUser: vi.fn(),
  updateUser: vi.fn(),
}));
vi.mock("@/components/Table", () => ({
  buildTable: (options: { data: unknown[] }) => ({ rows: options.data }),
  Table: (props: {
    table: { rows: Array<{ id: string }> };
    emptyState: unknown;
    onRowClick?: (index: number, row: { id: string }) => void;
    dense?: boolean;
    paginationPosition?: string;
  }) => (
    <>
      {pageState.showEmptyState
        ? props.emptyState
        : props.table.rows.map((row, index) => (
            <button onClick={() => props.onRowClick?.(index, row)}>
              {row.id}
            </button>
          ))}
      <span
        data-testid="table-props"
        data-dense={String(props.dense)}
        data-pagination-position={props.paginationPosition}
      />
    </>
  ),
}));
vi.mock("@/components/accounts/AddUser", () => ({
  AddUser: () => <h2>Add new user</h2>,
}));

import {
  AccountsPage,
  AccountToolbar,
} from "@/components/accounts/AccountsPage";
import { FilterBar } from "@/components/FilterBar";
import type { UserJson } from "@bindings/UserJson";

afterEach(cleanup);

const account = {
  id: "account-1",
  email: "ada@example.com",
  username: "ada",
  unverified_email: null,
  admin: false,
  provider_id: 0n,
  provider_user_id: null,
  created: 1_700_000_000n,
  updated: 1_700_000_000n,
} satisfies UserJson;

describe("AccountToolbar", () => {
  it("switches between simple and advanced account filters", async () => {
    const onModeChange = vi.fn();
    render(() => (
      <AccountToolbar advanced={false} onModeChange={onModeChange}>
        <input aria-label="Search accounts" />
      </AccountToolbar>
    ));

    expect(
      screen.getByRole("button", { name: "Search accounts" }),
    ).toBeVisible();
    expect(
      screen.getByRole("button", { name: "Search accounts" }),
    ).toHaveAttribute("aria-pressed", "true");
    expect(
      screen.getByRole("button", { name: "Advanced account filter" }),
    ).toHaveAttribute("aria-pressed", "false");
    expect(
      screen.getByRole("textbox", { name: "Search accounts" }),
    ).toBeVisible();
    await fireEvent.click(
      screen.getByRole("button", { name: "Advanced account filter" }),
    );
    expect(onModeChange).toHaveBeenCalledWith(true);
  });

  it("supports switching back to simple search", async () => {
    const onModeChange = vi.fn();
    render(() => (
      <AccountToolbar advanced={true} onModeChange={onModeChange}>
        <input aria-label="Advanced account filter" />
      </AccountToolbar>
    ));
    expect(
      screen.getByRole("button", { name: "Advanced account filter" }),
    ).toHaveAttribute("aria-pressed", "true");
    expect(
      screen.getByRole("button", { name: "Search accounts" }),
    ).toHaveAttribute("aria-pressed", "false");
    await fireEvent.click(
      screen.getByRole("button", { name: "Search accounts" }),
    );
    expect(onModeChange).toHaveBeenCalledWith(false);
  });

  it("keeps the active mode and its apply/clear actions accessible", async () => {
    const onModeChange = vi.fn();
    const onSubmit = vi.fn();
    render(() => (
      <AccountToolbar advanced={false} onModeChange={onModeChange}>
        <FilterBar initial="ada" label="Search accounts" onSubmit={onSubmit} />
        <button
          aria-label="Refresh accounts"
          onClick={() => onSubmit("refresh")}
        >
          Refresh
        </button>
        <button onClick={() => onSubmit("add")}>Add account</button>
      </AccountToolbar>
    ));

    expect(
      screen.getByRole("textbox", { name: "Search accounts" }),
    ).toBeVisible();
    expect(screen.getByRole("button", { name: "Apply filter" })).toBeVisible();
    await fireEvent.click(screen.getByRole("button", { name: "Apply filter" }));
    await fireEvent.click(screen.getByRole("button", { name: "Clear filter" }));
    await fireEvent.click(
      screen.getByRole("button", { name: "Refresh accounts" }),
    );
    await fireEvent.click(screen.getByRole("button", { name: "Add account" }));
    expect(onSubmit).toHaveBeenNthCalledWith(1, "ada");
    expect(onSubmit).toHaveBeenNthCalledWith(2, "");
    expect(onSubmit).toHaveBeenNthCalledWith(3, "refresh");
    expect(onSubmit).toHaveBeenNthCalledWith(4, "add");
  });
});

describe("AccountsPage integration", () => {
  beforeEach(() => {
    Object.assign(pageState.params, {
      search: "ada",
      filter: "admin = TRUE",
      advanced: "false",
      pageSize: "25",
      pageIndex: "2",
    });
    pageState.setSearchParams.mockReset();
    pageState.fetchUsers.mockReset().mockResolvedValue({ users: [] });
    pageState.invalidateQueries.mockReset();
    pageState.queryError = false;
    pageState.queryLoading = false;
    pageState.queryData = { users: [] };
    pageState.setDataVersion = undefined;
    pageState.showEmptyState = false;
  });

  it("renders the page shell with a persistent toolbar and add action", async () => {
    const { container } = render(() => <AccountsPage />);

    expect(container.firstElementChild).toHaveClass(
      "flex",
      "h-full",
      "min-h-0",
      "flex-col",
    );
    expect(screen.getByRole("heading", { name: "Accounts" })).toBeVisible();
    expect(screen.getAllByRole("button", { name: "Add account" })[0]).toBeVisible();
    expect(
      screen.getByRole("textbox", { name: "Search accounts" }),
    ).toBeVisible();
  });

  it("distinguishes no matches from no accounts using the active mode", async () => {
    pageState.showEmptyState = true;
    render(() => <AccountsPage />);
    expect(
      screen.getByText("No accounts match the current search or filter."),
    ).toBeVisible();

    cleanup();
    pageState.params.search = undefined;
    pageState.params.filter = undefined;
    render(() => <AccountsPage />);
    expect(screen.getByText("No accounts yet.")).toBeVisible();
  });

  it("uses the management description while loading and passes dense bottom pagination", () => {
    pageState.queryLoading = true;
    render(() => <AccountsPage />);
    expect(screen.getByText("Manage authentication identities and access")).toBeVisible();
    expect(screen.getByTestId("table-props")).toHaveAttribute("data-dense", "true");
    expect(screen.getByTestId("table-props")).toHaveAttribute("data-pagination-position", "bottom");
  });

  it("opens the account sheet on row activation and clears stale selection", async () => {
    pageState.queryData = { users: [account] };
    render(() => <AccountsPage />);
    await fireEvent.click(screen.getByRole("button", { name: account.id }));
    expect(await screen.findByText("Edit User")).toBeVisible();

    pageState.queryData = { users: [] };
    pageState.setDataVersion?.();
    await waitFor(() => expect(screen.queryByText("Edit User")).not.toBeInTheDocument());
  });

  it("shows a safe retry state when loading accounts fails", async () => {
    pageState.queryError = true;
    render(() => <AccountsPage />);
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Unable to load accounts.",
    );
    expect(screen.getByRole("alert")).not.toHaveTextContent("backend boom");
    await fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(pageState.invalidateQueries).toHaveBeenCalledWith({
      queryKey: ["users"],
    });
  });

  it("queries simple and advanced state with URL pagination", async () => {
    render(() => <AccountsPage />);
    await waitFor(() => expect(pageState.fetchUsers).toHaveBeenCalled());
    expect(pageState.fetchUsers).toHaveBeenLastCalledWith(
      undefined,
      25,
      2,
      undefined,
      "ada",
    );

    expect(pageState.params.advanced).toBe("false");
    await fireEvent.click(
      screen.getByRole("button", { name: "Advanced account filter" }),
    );
    await waitFor(() =>
      expect(pageState.fetchUsers).toHaveBeenLastCalledWith(
        "admin = TRUE",
        25,
        2,
        undefined,
        undefined,
      ),
    );

    expect(pageState.params.search).toBe("ada");
    expect(pageState.params.advanced).toBe("true");
    expect(pageState.params.filter).toBe("admin = TRUE");
    expect(pageState.params.pageSize).toBe("25");
    expect(pageState.params.pageIndex).toBe("2");
  });

  it("resets page on apply, preserves values while switching modes, and refreshes", async () => {
    render(() => <AccountsPage />);
    const filter = screen.getByRole("textbox", { name: "Search accounts" });
    await fireEvent.input(filter, { target: { value: "grace" } });
    await fireEvent.click(screen.getByRole("button", { name: "Apply filter" }));
    expect(pageState.params.search).toBe("grace");
    expect(pageState.params.pageIndex).toBe("0");

    await fireEvent.click(
      screen.getByRole("button", { name: "Advanced account filter" }),
    );
    expect(pageState.params.advanced).toBe("true");
    expect(pageState.params.search).toBe("grace");
    expect(pageState.params.filter).toBe("admin = TRUE");
    const advanced = screen.getByRole("textbox", {
      name: "Advanced account filter",
    });
    expect(advanced).toHaveValue("admin = TRUE");
    await fireEvent.input(advanced, { target: { value: "email ~ %" } });
    await fireEvent.click(screen.getByRole("button", { name: "Apply filter" }));
    expect(pageState.params.filter).toBe("email ~ %");
    expect(pageState.params.pageIndex).toBe("0");

    const callsBeforeRefresh = pageState.setSearchParams.mock.calls.length;
    await fireEvent.click(
      screen.getByRole("button", { name: "Refresh accounts" }),
    );
    expect(pageState.invalidateQueries).toHaveBeenCalledWith({
      queryKey: ["users"],
    });
    expect(pageState.setSearchParams).toHaveBeenCalledTimes(callsBeforeRefresh);

    await fireEvent.click(
      screen.getByRole("button", { name: "Search accounts" }),
    );
    expect(pageState.params.advanced).toBe("false");
    expect(
      screen.getByRole("textbox", { name: "Search accounts" }),
    ).toHaveValue("grace");
    expect(pageState.params.filter).toBe("email ~ %");
    await fireEvent.click(screen.getAllByRole("button", { name: "Add account" })[0]);
    expect(
      await screen.findByRole("heading", { name: "Add new user" }),
    ).toBeVisible();
  });
});
