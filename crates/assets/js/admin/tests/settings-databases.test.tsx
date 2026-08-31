import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { JSX } from "solid-js";
import { Config, DatabaseConfig } from "@proto/config";

type ChildrenProps = { children?: JSX.Element };
type DialogRootProps = ChildrenProps & {
  open: boolean;
  onOpenChange: (open: boolean) => void;
};
type DialogContentProps = ChildrenProps & { closeDisabled?: boolean };

const state = vi.hoisted(() => ({
  config: undefined as Config | undefined,
  configLoading: false,
  configError: false,
  info: { postgres: false },
  infoLoading: false,
  infoError: false,
  setConfig: vi.fn(),
  postSubmit: vi.fn(),
  setDirty: vi.fn(),
  updateConfig: undefined as ((config: Config) => void) | undefined,
  updateInfo: undefined as ((info: { postgres: boolean }) => void) | undefined,
  setConfigLoading: undefined as ((value: boolean) => void) | undefined,
  setInfoLoading: undefined as ((value: boolean) => void) | undefined,
  setConfigError: undefined as ((value: boolean) => void) | undefined,
  setInfoError: undefined as ((value: boolean) => void) | undefined,
}));

vi.mock("@tanstack/solid-query", () => ({
  useQueryClient: () => ({
    getQueryData: () => ({ hash: "cached-hash" }),
    invalidateQueries: vi.fn().mockResolvedValue(undefined),
  }),
}));
vi.mock("@/lib/api/config", async () => {
  const { createSignal } =
    await vi.importActual<typeof import("solid-js")>("solid-js");
  return {
    createConfigQuery: () => {
      const [data, setData] = createSignal(
        state.config ? { config: state.config } : undefined,
      );
      state.updateConfig = (config) => setData({ config });
      const [loading, setLoading] = createSignal(state.configLoading);
      const [error, setError] = createSignal(state.configError);
      state.setConfigLoading = setLoading;
      state.setConfigError = setError;
      return {
        get data() {
          return data();
        },
        get isLoading() {
          return loading();
        },
        get isError() {
          return error();
        },
        error: new Error("raw config detail"),
      };
    },
    setConfig: state.setConfig,
  };
});
vi.mock("@/lib/api/info", async () => {
  const { createSignal } =
    await vi.importActual<typeof import("solid-js")>("solid-js");
  return {
    createSystemInfoQuery: () => {
      const [data, setData] = createSignal(state.info);
      state.updateInfo = setData;
      const [loading, setLoading] = createSignal(state.infoLoading);
      const [error, setError] = createSignal(state.infoError);
      state.setInfoLoading = setLoading;
      state.setInfoError = setError;
      return {
        get data() {
          return data();
        },
        get isLoading() {
          return loading();
        },
        get isError() {
          return error();
        },
        error: new Error("raw info detail"),
      };
    },
  };
});
vi.mock("@/components/ui/dialog", async () => {
  const Solid = await vi.importActual<typeof import("solid-js")>("solid-js");
  const Context = Solid.createContext<{
    open: () => boolean;
    setOpen: (open: boolean) => void;
  }>();
  return {
    Dialog: (props: DialogRootProps) => (
      <Context.Provider
        value={{
          open: () => props.open,
          setOpen: (open) => props.onOpenChange(open),
        }}
      >
        {props.children}
      </Context.Provider>
    ),
    DialogContent: (props: DialogContentProps) => {
      const context = Solid.useContext(Context)!;
      return (
        <Solid.Show when={context.open()}>
          <div role="dialog">
            {props.children}
            <button
              type="button"
              aria-label="Close"
              disabled={props.closeDisabled}
              onClick={() => context.setOpen(false)}
            >
              Close
            </button>
          </div>
        </Solid.Show>
      );
    },
    DialogHeader: (props: ChildrenProps) => <div>{props.children}</div>,
    DialogTitle: (props: ChildrenProps) => <h2>{props.children}</h2>,
    DialogFooter: (props: ChildrenProps) => <div>{props.children}</div>,
    DialogTrigger: (props: ChildrenProps) => {
      const context = Solid.useContext(Context)!;
      return (
        <span onClick={() => context.setOpen(true)}>{props.children}</span>
      );
    },
  };
});
vi.mock("@/components/ui/toast", () => ({ showToast: vi.fn() }));

import {
  DatabaseSettings,
  validateDatabaseName,
} from "@/components/settings/DatabaseSettings";

const baseConfig = (databases: string[] = []) =>
  Config.fromPartial({
    server: { applicationName: "Keep this app" },
    databases: databases.map((name) => DatabaseConfig.fromPartial({ name })),
  });
const setup = (config = baseConfig(["analytics"])) => {
  state.config = config;
  state.configLoading = false;
  state.configError = false;
  state.info = { postgres: false };
  state.infoLoading = false;
  state.infoError = false;
  state.setConfig.mockReset().mockResolvedValue(undefined);
  state.postSubmit.mockReset();
  state.setDirty.mockReset();
  return render(() => (
    <DatabaseSettings setDirty={state.setDirty} postSubmit={state.postSubmit} />
  ));
};
const linkButton = () => screen.getByRole("button", { name: /^Link$/ });
const dialogButton = (name: string) =>
  within(screen.getByRole("dialog")).getByRole("button", { name });
const unlinkButton = () => screen.getByRole("button", { name: /^Unlink$/ });
const selectDatabase = (name: string) => {
  const root = screen.getByLabelText(`Select ${name}`);
  fireEvent.click(root.querySelector("input") ?? root);
};
const openLink = () => {
  fireEvent.click(linkButton());
  return screen.getByRole("dialog");
};

beforeEach(() => {
  state.config = undefined;
  state.configLoading = false;
  state.configError = false;
  state.infoLoading = false;
  state.infoError = false;
  state.setConfigLoading = undefined;
  state.setInfoLoading = undefined;
  state.setConfigError = undefined;
  state.setInfoError = undefined;
});
afterEach(cleanup);

describe("database settings states", () => {
  it("shows system information loading status", () => {
    state.config = baseConfig();
    setup(state.config);
    state.setInfoLoading?.(true);
    expect(screen.getByRole("status")).toHaveTextContent(/loading database/i);
  });
  it("shows config loading independently", () => {
    setup();
    state.setConfigLoading?.(true);
    expect(screen.getByRole("status")).toHaveTextContent(/loading database/i);
  });
  it("shows generic system info errors without raw details", () => {
    setup();
    state.setInfoError?.(true);
    expect(screen.getByRole("alert")).toHaveTextContent(
      /unable to load system/i,
    );
    expect(screen.queryByText(/raw info detail/i)).toBeNull();
  });
  it("shows generic config errors without raw details", () => {
    setup();
    state.setConfigError?.(true);
    expect(screen.getByRole("alert")).toHaveTextContent(
      /unable to load configuration/i,
    );
    expect(screen.queryByText(/raw config detail/i)).toBeNull();
  });
  it("shows Postgres unsupported state without waiting for config", () => {
    setup();
    state.setConfigLoading?.(true);
    state.updateInfo?.({ postgres: true });
    expect(screen.getByText(/not supported in postgres/i)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Link$/ })).toBeNull();
    expect(screen.queryByRole("button", { name: /^Unlink$/ })).toBeNull();
  });
  it("shows empty SQLite state and import/export information", () => {
    setup(baseConfig());
    expect(screen.getByText(/no linked databases/i)).toBeInTheDocument();
    expect(screen.getByText(/data import & export/i)).toBeInTheDocument();
    expect(screen.getAllByText(/sqlite3/i).length).toBeGreaterThan(0);
  });
  it("renders semantic names, overflow containment, and functional select all", () => {
    setup(baseConfig(["analytics", "events"]));
    expect(screen.getByRole("table")).toBeInTheDocument();
    expect(screen.getByText("analytics")).toBeInTheDocument();
    expect(screen.getByText("events")).toBeInTheDocument();
    expect(document.querySelector(".overflow-x-auto")).toBeTruthy();
    expect(screen.getByLabelText("Select analytics")).toBeInTheDocument();
    expect(screen.getByLabelText("Select events")).toBeInTheDocument();
    const selectAll = screen.getByLabelText("Select all databases");
    fireEvent.click(selectAll.querySelector("input") ?? selectAll);
    expect(unlinkButton()).not.toBeDisabled();
    fireEvent.click(unlinkButton());
    expect(screen.getByRole("dialog")).toHaveTextContent("analytics, events");
  });

  it("clears stale selection after an incoming config refresh", async () => {
    setup(baseConfig(["analytics"]));
    selectDatabase("analytics");
    expect(unlinkButton()).not.toBeDisabled();
    state.updateConfig?.(baseConfig(["remote"]));
    await waitFor(() => expect(unlinkButton()).toBeDisabled());
  });
});

describe("database link lifecycle", () => {
  it("opens a labelled dialog, cancels, and restores trigger focus", async () => {
    setup();
    const trigger = linkButton();
    trigger.focus();
    const dialog = openLink();
    expect(dialog).toHaveTextContent("Link Database");
    expect(screen.getByLabelText("Name")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    await waitFor(() => expect(trigger).toHaveFocus());
    expect(state.setConfig).not.toHaveBeenCalled();
  });
  it.each([
    "",
    "   ",
    "main",
    "public",
    "logs",
    "session",
    "bad name",
    "a/b",
    "analytics",
  ])("rejects invalid name %j without API", (value) => {
    setup();
    openLink();
    const input = screen.getByLabelText("Name");
    fireEvent.input(input, { target: { value } });
    expect(dialogButton("Link")).toBeDisabled();
    expect(state.setConfig).not.toHaveBeenCalled();
  });
  it("accepts and trims a valid name", async () => {
    setup();
    openLink();
    fireEvent.input(screen.getByLabelText("Name"), {
      target: { value: " metrics " },
    });
    expect(dialogButton("Link")).not.toBeDisabled();
    fireEvent.click(dialogButton("Link"));
    await waitFor(() => expect(state.setConfig).toHaveBeenCalled());
    expect(state.setConfig).toHaveBeenCalledWith(
      expect.objectContaining({ throw: true, config: expect.anything() }),
    );
    const saved = state.setConfig.mock.calls[0][0].config as Config;
    expect(saved.databases.map((db) => db.name)).toEqual([
      "analytics",
      "metrics",
    ]);
  });
  it("disables name editing and dialog close while link is pending", async () => {
    let resolve!: () => void;
    state.setConfig.mockReturnValue(new Promise<void>((r) => (resolve = r)));
    setup();
    openLink();
    const input = screen.getByLabelText("Name");
    fireEvent.input(input, { target: { value: "metrics" } });
    fireEvent.click(dialogButton("Link"));
    expect(input).toBeDisabled();
    expect(screen.getByRole("button", { name: "Close" })).toBeDisabled();
    resolve();
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("re-enables link dialog close after failure", async () => {
    setup();
    state.setConfig.mockRejectedValue(new Error("secret"));
    openLink();
    fireEvent.input(screen.getByLabelText("Name"), {
      target: { value: "metrics" },
    });
    fireEvent.click(dialogButton("Link"));
    await waitFor(() => expect(screen.getByRole("alert")).toBeInTheDocument());
    expect(screen.getByRole("button", { name: "Close" })).not.toBeDisabled();
  });

  it("keeps link dialog and value while pending, then closes after success", async () => {
    let resolve!: () => void;
    state.setConfig.mockReturnValue(new Promise<void>((r) => (resolve = r)));
    setup();
    openLink();
    const input = screen.getByLabelText("Name");
    fireEvent.input(input, { target: { value: " metrics " } });
    fireEvent.click(dialogButton("Link"));
    expect(screen.getByRole("button", { name: "Linking…" })).toBeDisabled();
    expect(input).toHaveValue(" metrics ");
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    resolve();
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(state.postSubmit).toHaveBeenCalledOnce();
  });
  it("keeps link open on failure and hides raw error", async () => {
    setup();
    state.setConfig.mockRejectedValue(new Error("secret backend detail"));
    openLink();
    fireEvent.input(screen.getByLabelText("Name"), {
      target: { value: "metrics" },
    });
    fireEvent.click(dialogButton("Link"));
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(/unable to link/i),
    );
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(screen.queryByText(/secret backend detail/i)).toBeNull();
  });
  it("preserves unrelated config and incoming databases present before link", async () => {
    setup(baseConfig(["analytics"]));
    openLink();
    fireEvent.input(screen.getByLabelText("Name"), {
      target: { value: "metrics" },
    });
    state.updateConfig?.(baseConfig(["analytics", "remote"]));
    fireEvent.click(dialogButton("Link"));
    await waitFor(() => expect(state.postSubmit).toHaveBeenCalledOnce());
    const saved = state.setConfig.mock.calls[0][0].config as Config;
    expect(saved.server?.applicationName).toBe("Keep this app");
    expect(saved.databases.map((db) => db.name)).toEqual([
      "analytics",
      "remote",
      "metrics",
    ]);
  });
  it("does not callback after link unmount", async () => {
    let resolve!: () => void;
    state.setConfig.mockReturnValue(new Promise<void>((r) => (resolve = r)));
    const view = setup();
    openLink();
    fireEvent.input(screen.getByLabelText("Name"), {
      target: { value: "metrics" },
    });
    fireEvent.click(dialogButton("Link"));
    view.unmount();
    resolve();
    await Promise.resolve();
    expect(state.postSubmit).not.toHaveBeenCalled();
  });
});

describe("database unlink lifecycle", () => {
  it("requires selection, confirms names, and restores trigger focus", async () => {
    setup(baseConfig(["analytics", "events"]));
    expect(unlinkButton()).toBeDisabled();
    selectDatabase("analytics");
    selectDatabase("events");
    expect(unlinkButton()).not.toBeDisabled();
    const trigger = unlinkButton();
    trigger.focus();
    fireEvent.click(trigger);
    expect(screen.getByRole("dialog")).toHaveTextContent("analytics, events");
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(unlinkButton()).not.toBeDisabled();
    await waitFor(() => expect(trigger).toHaveFocus());
    expect(state.setConfig).not.toHaveBeenCalled();
  });
  it("keeps unlink dialog and selection on failure without raw details", async () => {
    setup(baseConfig(["analytics"]));
    state.setConfig.mockRejectedValue(new Error("secret unlink detail"));
    selectDatabase("analytics");
    fireEvent.click(unlinkButton());
    fireEvent.click(screen.getByRole("button", { name: "Confirm" }));
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(/unable to unlink/i),
    );
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(screen.getByLabelText("Select analytics")).toHaveAttribute(
      "data-checked",
      "",
    );
    expect(screen.queryByText(/secret unlink detail/i)).toBeNull();
  });
  it("waits for unlink success and preserves incoming unselected databases", async () => {
    let resolve!: () => void;
    state.setConfig.mockReturnValue(new Promise<void>((r) => (resolve = r)));
    setup(baseConfig(["analytics", "events"]));
    selectDatabase("analytics");
    fireEvent.click(unlinkButton());
    state.updateConfig?.(baseConfig(["analytics", "events", "remote"]));
    fireEvent.click(screen.getByRole("button", { name: "Confirm" }));
    expect(screen.getByRole("button", { name: "Unlinking…" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Close" })).toBeDisabled();
    resolve();
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(state.postSubmit).toHaveBeenCalledOnce();
    const saved = state.setConfig.mock.calls[0][0].config as Config;
    expect(saved.databases.map((db) => db.name)).toEqual(["events", "remote"]);
  });
  it("does not mutate an already-sent unlink after a later refresh", async () => {
    let resolve!: () => void;
    state.setConfig.mockReturnValue(new Promise<void>((r) => (resolve = r)));
    setup(baseConfig(["analytics", "events"]));
    selectDatabase("analytics");
    fireEvent.click(unlinkButton());
    fireEvent.click(screen.getByRole("button", { name: "Confirm" }));
    await waitFor(() => expect(state.setConfig).toHaveBeenCalledOnce());
    const sent = state.setConfig.mock.calls[0][0].config as Config;
    expect(sent.databases.map((db) => db.name)).toEqual(["events"]);
    state.updateConfig?.(baseConfig(["analytics", "events", "remote"]));
    expect(state.setConfig).toHaveBeenCalledOnce();
    expect(sent.databases.map((db) => db.name)).toEqual(["events"]);
    resolve();
    await Promise.resolve();
  });

  it("does not callback after unlink unmount", async () => {
    let resolve!: () => void;
    state.setConfig.mockReturnValue(new Promise<void>((r) => (resolve = r)));
    const view = setup(baseConfig(["analytics"]));
    selectDatabase("analytics");
    fireEvent.click(unlinkButton());
    fireEvent.click(screen.getByRole("button", { name: "Confirm" }));
    view.unmount();
    resolve();
    await Promise.resolve();
    expect(state.postSubmit).not.toHaveBeenCalled();
  });
});

describe("database validation", () => {
  it.each([
    ["", "Enter a database name."],
    ["   ", "Enter a database name."],
    ["main", "That database name is reserved."],
    ["analytics", "That database is already linked."],
    ["has space", "Use only letters, numbers, underscores, and hyphens."],
  ])("rejects %j", (name, error) => {
    expect(
      validateDatabaseName(name, [
        DatabaseConfig.fromPartial({ name: "analytics" }),
      ]),
    ).toBe(error);
  });
  it.each(["metrics", "metrics_2", "metrics-prod", "A1"])(
    "accepts %s",
    (name) => {
      expect(validateDatabaseName(name, [])).toBeUndefined();
    },
  );
});
