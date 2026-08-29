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
  createUser: vi.fn(),
  updateUser: vi.fn(),
  deleteUser: vi.fn(),
  mintTokens: vi.fn(),
  copyToClipboard: vi.fn(),
  invalidateQueries: vi.fn(),
  queryError: false,
  queryLoading: false,
  queryData: undefined as
    | { users: Array<{ id: string; email?: string }>; total_row_count?: bigint }
    | undefined,
  setDataVersion: undefined as (() => void) | undefined,
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
  createUser: pageState.createUser,
  deleteUser: pageState.deleteUser,
  updateUser: pageState.updateUser,
}));
vi.mock("@/lib/api/mint", () => ({
  mintTokens: pageState.mintTokens,
}));
vi.mock("@/lib/utils", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/utils")>();
  return { ...actual, copyToClipboard: pageState.copyToClipboard };
});
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
    pageState.createUser.mockReset();
    pageState.updateUser.mockReset();
    pageState.deleteUser.mockReset();
    pageState.mintTokens.mockReset();
    pageState.copyToClipboard.mockReset();
    pageState.invalidateQueries.mockReset();
    pageState.queryError = false;
    pageState.queryLoading = false;
    pageState.queryData = { users: [], total_row_count: 0n };
    pageState.setDataVersion = undefined;
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

  it("distinguishes no matches from no accounts using response data and the active mode", async () => {
    render(() => <AccountsPage />);
    expect(
      screen.getByText("No accounts match the current search or filter."),
    ).toBeVisible();
    await waitFor(() =>
      expect(pageState.fetchUsers).toHaveBeenLastCalledWith(
        undefined,
        25,
        2,
        undefined,
        "ada",
      ),
    );

    cleanup();
    pageState.params.search = undefined;
    pageState.params.filter = undefined;
    render(() => <AccountsPage />);
    expect(screen.getByText("No accounts yet.")).toBeVisible();
    expect(screen.getAllByText("Add account")).toHaveLength(1);
  });

  it("keeps the toolbar and add action mounted while loading", () => {
    pageState.queryLoading = true;
    pageState.queryData = undefined;
    render(() => <AccountsPage />);
    expect(
      screen.getByText("Manage authentication identities and access"),
    ).toBeVisible();
    expect(
      screen.getByRole("textbox", { name: "Search accounts" }),
    ).toBeVisible();
    expect(screen.getByRole("button", { name: "Add account" })).toBeVisible();
    expect(screen.getByRole("table")).toBeVisible();
  });

  it("reports add failures while keeping the sheet open and refetching", async () => {
    pageState.createUser.mockRejectedValueOnce(
      new Error("email already exists"),
    );
    render(() => <AccountsPage />);
    await fireEvent.click(screen.getByRole("button", { name: "Add account" }));
    const email = screen.getByRole("textbox", { name: "Email" });
    const password = screen.getByLabelText("Password");
    await fireEvent.change(email, { target: { value: "ada@example.com" } });
    await fireEvent.blur(email);
    await fireEvent.change(password, { target: { value: "password" } });
    await fireEvent.blur(password);
    const add = screen.getByRole("button", { name: "Add" });
    await waitFor(() => expect(add).not.toBeDisabled());
    await fireEvent.click(add);

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "email already exists",
    );
    expect(screen.getByRole("heading", { name: "Add new user" })).toBeVisible();
    expect(
      screen.getByText("Create a new user account and configure its access."),
    ).toBeVisible();
    expect(pageState.invalidateQueries).toHaveBeenCalledWith({
      queryKey: ["users"],
    });
  });

  it("reports edit failures while keeping the sheet open and refetching", async () => {
    pageState.queryData = { users: [account] };
    pageState.updateUser.mockRejectedValueOnce(new Error("update rejected"));
    render(() => <AccountsPage />);
    const row = screen
      .getAllByRole("row")
      .find((candidate) => candidate.textContent?.includes("ada@example.com"));
    await fireEvent.keyDown(row!, { key: "Enter" });
    await fireEvent.click(screen.getByRole("button", { name: "Submit" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "update rejected",
    );
    expect(screen.getByText("Edit User")).toBeVisible();
    expect(pageState.invalidateQueries).toHaveBeenCalledWith({
      queryKey: ["users"],
    });
  });

  it("reports login token copy failures in the edit sheet", async () => {
    pageState.queryData = { users: [account] };
    pageState.mintTokens.mockRejectedValueOnce(new Error("token failed"));
    render(() => <AccountsPage />);
    const row = screen
      .getAllByRole("row")
      .find((candidate) => candidate.textContent?.includes("ada@example.com"));
    await fireEvent.keyDown(row!, { key: "Enter" });
    await fireEvent.click(
      screen.getByRole("button", { name: "Copy login tokens" }),
    );

    expect(await screen.findByRole("alert")).toHaveTextContent("token failed");
  });

  it("reports clipboard failures in the edit sheet", async () => {
    pageState.queryData = { users: [account] };
    pageState.mintTokens.mockResolvedValueOnce({ access_token: "token" });
    pageState.copyToClipboard.mockRejectedValueOnce(
      new Error("clipboard failed"),
    );
    render(() => <AccountsPage />);
    const row = screen
      .getAllByRole("row")
      .find((candidate) => candidate.textContent?.includes("ada@example.com"));
    await fireEvent.keyDown(row!, { key: "Enter" });
    await fireEvent.click(
      screen.getByRole("button", { name: "Copy login tokens" }),
    );

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "clipboard failed",
    );
  });

  it("guards deletion in flight and clears failures when reopened", async () => {
    pageState.queryData = { users: [account] };
    let rejectDelete!: (reason: Error) => void;
    pageState.deleteUser.mockReturnValueOnce(
      new Promise((_resolve, reject) => {
        rejectDelete = reject;
      }),
    );
    render(() => <AccountsPage />);
    const row = screen
      .getAllByRole("row")
      .find((candidate) => candidate.textContent?.includes("ada@example.com"));
    await fireEvent.keyDown(row!, { key: "Enter" });
    const deleteButtons = screen.getAllByRole("button", { name: "Delete" });
    await fireEvent.click(deleteButtons[0]);
    const confirm = screen.getByRole("button", { name: "Delete" });
    await fireEvent.click(confirm);
    expect(confirm).toBeDisabled();
    expect(screen.getByRole("button", { name: "Deleting..." })).toBeDisabled();
    rejectDelete(new Error("delete failed"));
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent("delete failed"),
    );
    await fireEvent.click(screen.getByRole("button", { name: "Back" }));
    await fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("protects dirty edits with the controlled close confirmation", async () => {
    pageState.queryData = { users: [account] };
    render(() => <AccountsPage />);
    const row = screen
      .getAllByRole("row")
      .find((candidate) => candidate.textContent?.includes("ada@example.com"));
    await fireEvent.keyDown(row!, { key: "Enter" });
    await fireEvent.change(screen.getByRole("textbox", { name: "Email" }), {
      target: { value: "changed@example.com" },
    });
    await fireEvent.keyDown(document, { key: "Escape" });

    expect(await screen.findByText("Pending Changes")).toBeVisible();
    await fireEvent.click(screen.getByRole("button", { name: "Proceed" }));
    await waitFor(() =>
      expect(screen.queryByText("Edit User")).not.toBeInTheDocument(),
    );
  });

  it("opens the account sheet on row activation and clears stale selection", async () => {
    pageState.queryData = { users: [account] };
    render(() => <AccountsPage />);
    const row = screen
      .getAllByRole("row")
      .find((candidate) => candidate.textContent?.includes("ada@example.com"));
    expect(row).toBeDefined();
    await fireEvent.keyDown(row!, { key: "Enter" });
    expect(await screen.findByText("Edit User")).toBeVisible();

    pageState.queryData = { users: [] };
    pageState.setDataVersion?.();
    await waitFor(() =>
      expect(screen.queryByText("Edit User")).not.toBeInTheDocument(),
    );
  });

  it("shows a safe retry state with persistent workspace actions", async () => {
    pageState.queryError = true;
    pageState.queryData = undefined;
    render(() => <AccountsPage />);
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Unable to load accounts.",
    );
    expect(screen.getByRole("alert")).not.toHaveTextContent("backend boom");
    expect(
      screen.getByRole("textbox", { name: "Search accounts" }),
    ).toBeVisible();
    expect(screen.getByRole("button", { name: "Add account" })).toBeVisible();
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
