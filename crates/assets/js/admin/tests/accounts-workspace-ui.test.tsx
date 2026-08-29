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
    createEffect(() => void options().queryFn());
    return {
      isLoading: false,
      get isError() {
        return pageState.queryError;
      },
      data: { users: [] },
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
  buildTable: () => ({}),
  // eslint-disable-next-line solid/reactivity
  Table: (props: { emptyState: unknown }) =>
    pageState.showEmptyState ? props.emptyState : null,
}));
vi.mock("@/components/accounts/AddUser", () => ({
  AddUser: () => <h2>Add new user</h2>,
}));

import {
  AccountsPage,
  AccountToolbar,
} from "@/components/accounts/AccountsPage";
import { FilterBar } from "@/components/FilterBar";

afterEach(cleanup);

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
    expect(screen.getByRole("button", { name: "Add account" })).toBeVisible();
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

  it("shows a safe retry state when loading accounts fails", async () => {
    pageState.queryError = true;
    render(() => <AccountsPage />);
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Unable to load accounts.",
    );
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
    await fireEvent.click(screen.getByRole("button", { name: "Add account" }));
    expect(
      await screen.findByRole("heading", { name: "Add new user" }),
    ).toBeVisible();
  });
});
