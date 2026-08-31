import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import * as Solid from "solid-js";

const state = vi.hoisted(() => ({
  params: { group: "host" } as { group?: string },
  mobile: false,
  navigate: vi.fn(),
  invalidate: vi.fn(),
  childProps: undefined as any,
  config: undefined as any,
  configLoading: false,
  configError: false,
  info: undefined as any,
  infoLoading: false,
  infoError: false,
  setConfig: vi.fn(),
  showToast: vi.fn(),
  updateConfigQuery: undefined as undefined | ((config: any) => void),
}));

vi.mock("@tanstack/solid-query", () => ({ useQueryClient: () => ({}) }));
vi.mock("@solidjs/router", () => ({
  useParams: () => state.params,
  useNavigate: () => state.navigate,
}));
vi.mock("@/components/Header", () => ({
  Header: (p: any) => (
    <header>
      {p.leading}
      {p.title}
      {p.titleSelect}
      {p.left}
    </header>
  ),
}));
vi.mock("@/components/IconButton", () => ({
  IconButton: (p: any) => (
    <button aria-label={p["aria-label"]} onClick={p.onClick}>
      {p.children}
    </button>
  ),
}));
vi.mock("@/components/ui/dialog", () => ({
  Dialog: (p: any) => <>{p.children}</>,
}));
vi.mock("@/components/ui/toast", () => ({ showToast: state.showToast }));
vi.mock("@/components/SafeSheet", () => ({
  ConfirmCloseDialog: (p: any) => (
    <>
      <button onClick={p.back}>Cancel</button>
      <button onClick={p.confirm}>Confirm</button>
    </>
  ),
}));
vi.mock("@/components/ui/sidebar", () => ({
  SidebarProvider: (p: any) => <>{p.children}</>,
  Sidebar: (p: any) => <aside>{p.children}</aside>,
  SidebarContent: (p: any) => <>{p.children}</>,
  SidebarGroup: (p: any) => <>{p.children}</>,
  SidebarGroupContent: (p: any) => <>{p.children}</>,
  SidebarMenu: (p: any) => <nav>{p.children}</nav>,
  SidebarMenuItem: (p: any) => <>{p.children}</>,
  SidebarMenuButton: (p: any) => (
    <button onClick={p.onClick}>{p.children}</button>
  ),
  SidebarInset: (p: any) => <main>{p.children}</main>,
  SidebarRail: () => null,
  SidebarTrigger: () => <button aria-label="Toggle Sidebar">Sidebar</button>,
  useSidebar: () => ({ setOpenMobile: vi.fn() }),
}));
vi.mock("@/components/ui/button", () => ({
  Button: (p: any) => <button {...p}>{p.children}</button>,
}));
vi.mock("@/components/ui/card", () => ({
  Card: (p: any) => <div>{p.children}</div>,
  CardHeader: (p: any) => <div>{p.children}</div>,
  CardContent: (p: any) => <div>{p.children}</div>,
}));
vi.mock("@/components/ui/text-field", () => ({
  TextField: (p: any) => <div>{p.children}</div>,
  TextFieldLabel: (p: any) => <label>{p.children}</label>,
  TextFieldInput: (p: any) => <input {...p} onInput={p.onChange} />,
}));
vi.mock("@/lib/signals", () => ({ createIsMobile: () => () => state.mobile }));
vi.mock("@/lib/api/config", async () => {
  const { createSignal } =
    await vi.importActual<typeof import("solid-js")>("solid-js");
  return {
    createConfigQuery: () => {
      const [data, setData] = createSignal(
        state.config ? { config: state.config } : undefined,
      );
      state.updateConfigQuery = (config) => setData({ config });
      return {
        get data() {
          return data();
        },
        isLoading: state.configLoading,
        isError: state.configError,
        error: new Error("backend secret"),
      };
    },
    setConfig: state.setConfig,
    invalidateConfig: state.invalidate,
  };
});
vi.mock("@/lib/api/info", () => ({
  createSystemInfoQuery: () => ({
    data: state.info,
    isLoading: state.infoLoading,
    isError: state.infoError,
    isSuccess: !!state.info,
    error: new Error("backend secret"),
  }),
}));
vi.mock("@/components/settings/AuthSettings", () => ({
  AuthSettings: (p: any) => {
    state.childProps = p;
    return <div>AuthSettings</div>;
  },
}));
vi.mock("@/components/settings/BackupSettings", () => ({
  BackupSettings: (p: any) => {
    state.childProps = p;
    return <div>BackupSettings</div>;
  },
}));
vi.mock("@/components/settings/DatabaseSettings", () => ({
  DatabaseSettings: (p: any) => {
    state.childProps = p;
    return <div>DatabaseSettings</div>;
  },
}));
vi.mock("@/components/settings/EmailSettings", () => ({
  EmailSettings: (p: any) => {
    state.childProps = p;
    return <div>EmailSettings</div>;
  },
}));
vi.mock("@/components/settings/JobSettings", () => ({
  JobSettings: (p: any) => {
    state.childProps = p;
    return <div>JobSettings</div>;
  },
}));
vi.mock("@/components/settings/SchemaSettings", () => ({
  SchemaSettings: (p: any) => {
    state.childProps = p;
    return <div>SchemaSettings</div>;
  },
}));
vi.mock("@/components/Version", () => ({ Version: () => null }));

import { SettingsPage } from "@/components/settings/SettingsPage";
import { SettingsFormActions } from "@/components/settings/SettingsFormActions";

afterEach(cleanup);
beforeEach(() => {
  state.params.group = "email";
  state.mobile = false;
  state.navigate.mockReset();
  state.invalidate.mockReset();
  state.childProps = undefined;
  state.config = undefined;
  state.configLoading = false;
  state.configError = false;
  state.info = undefined;
  state.infoLoading = false;
  state.infoError = false;
  state.setConfig.mockReset();
  state.setConfig.mockResolvedValue(undefined);
  state.showToast.mockReset();
  state.updateConfigQuery = undefined;
});

describe("settings workspace", () => {
  it("renders every visible category label with unchanged route destinations", () => {
    render(() => <SettingsPage />);
    for (const label of [
      "General",
      "Email",
      "Authentication",
      "Backups",
      "Jobs",
      "Databases",
      "Schemas",
    ])
      expect(screen.getByText(label)).toBeInTheDocument();
    for (const [label, route] of [
      ["General", "host"],
      ["Email", "email"],
      ["Authentication", "auth"],
      ["Backups", "backup"],
      ["Jobs", "jobs"],
      ["Databases", "data"],
      ["Schemas", "schema"],
    ]) {
      cleanup();
      state.params.group = route === "host" ? "email" : "host";
      render(() => <SettingsPage />);
      fireEvent.click(screen.getByText(label));
      expect(state.navigate).toHaveBeenLastCalledWith(`/settings/${route}`);
    }
  });
  it("falls invalid routes back to General", () => {
    state.params.group = "invalid";
    render(() => <SettingsPage />);
    expect(screen.getByText("General")).toBeInTheDocument();
  });
  it("keeps mobile trigger and desktop sidebar, and refresh is named", () => {
    render(() => <SettingsPage />);
    expect(screen.getByRole("complementary")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Toggle Sidebar" }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Refresh settings" }));
    expect(state.invalidate).toHaveBeenCalled();
    expect(document.querySelector(".max-w-5xl")).not.toHaveClass(
      "overflow-y-auto",
    );
    state.mobile = true;
    cleanup();
    render(() => <SettingsPage />);
    expect(
      screen.getByRole("button", { name: "Toggle Sidebar" }),
    ).toBeInTheDocument();
  });
  it("opens confirmation for dirty navigation and confirming clears dirty and navigates", () => {
    render(() => <SettingsPage />);
    state.childProps.setDirty(true);
    fireEvent.click(screen.getByText("Authentication"));
    expect(screen.getByText("Confirm")).toBeInTheDocument();
    fireEvent.click(screen.getByText("Confirm"));
    expect(state.navigate).toHaveBeenLastCalledWith("/settings/auth");
    fireEvent.click(screen.getByText("Jobs"));
    expect(state.navigate).toHaveBeenLastCalledWith("/settings/jobs");
  });
  it("allows navigation after a dirty form is reverted", () => {
    render(() => <SettingsPage />);
    state.childProps.setDirty(true);
    state.childProps.setDirty(false);
    fireEvent.click(screen.getByText("Jobs"));
    expect(state.navigate).toHaveBeenLastCalledWith("/settings/jobs");
  });
  it("renders actions only dirty, disables submission, and invokes Reset", () => {
    const reset = vi.fn();
    render(() => (
      <SettingsFormActions
        dirty={false}
        canSubmit={true}
        isSubmitting={false}
        onReset={reset}
      />
    ));
    expect(screen.queryByRole("button", { name: "Save changes" })).toBeNull();
    cleanup();
    render(() => (
      <SettingsFormActions
        dirty={true}
        canSubmit={true}
        isSubmitting={false}
        onReset={reset}
      />
    ));
    fireEvent.click(screen.getByRole("button", { name: "Reset" }));
    expect(reset).toHaveBeenCalled();
    cleanup();
    render(() => (
      <SettingsFormActions
        dirty={true}
        canSubmit={true}
        isSubmitting={true}
        onReset={reset}
      />
    ));
    expect(screen.getByRole("button", { name: "Saving…" })).toBeDisabled();
    expect(screen.getByRole("status")).toHaveTextContent("Saving…");
  });
});

const generalConfig = () => ({
  server: {
    applicationName: "Trailbase",
    siteUrl: "https://example.test",
    logsRetentionSec: 3600n,
  },
});
const runtimeInfo = () => ({
  threads: 4,
  compiler: "rustc",
  commit_hash: "abcdef123456",
  commit_date: "today",
  start_time: String(Date.now() / 1000),
  command_line_arguments: ["trailbase"],
  postgres: true,
});

describe("General settings integration", () => {
  beforeEach(() => {
    state.params.group = "host";
    state.config = generalConfig();
    state.info = runtimeInfo();
  });

  it("renders runtime information as semantic pairs and editable fields", () => {
    render(() => <SettingsPage />);
    expect(
      screen.getByText("CPU Threads:", { selector: "dt" }),
    ).toBeInTheDocument();
    expect(screen.getByText("4", { selector: "dd" })).toBeInTheDocument();
    expect(screen.getByText("Postgres:")).toBeInTheDocument();
    expect(screen.getByText("enabled")).toBeInTheDocument();
    const runtime = document.querySelector("dl")!;
    expect(runtime.querySelectorAll("dt")).toHaveLength(8);
    expect(runtime.querySelectorAll("dd")).toHaveLength(8);
    expect(screen.getAllByTestId("input")[0]).toHaveValue("Trailbase");
    expect(screen.getAllByTestId("input")[1]).toHaveValue(
      "https://example.test",
    );
    expect(screen.getAllByTestId("input")[2]).toHaveValue("3600");
    expect(screen.queryByRole("button", { name: "Save changes" })).toBeNull();
  });

  it("uses generic loading and error messages", () => {
    state.configLoading = true;
    state.infoLoading = true;
    render(() => <SettingsPage />);
    expect(screen.getByText("Loading settings...")).toBeInTheDocument();
    expect(
      screen.getByText("Loading runtime information..."),
    ).toBeInTheDocument();
    cleanup();
    state.configLoading = false;
    state.infoLoading = false;
    state.configError = true;
    state.infoError = true;
    render(() => <SettingsPage />);
    expect(screen.getByText("Unable to load settings")).toBeInTheDocument();
    expect(
      screen.getByText("Unable to load runtime information"),
    ).toBeInTheDocument();
    expect(screen.queryByText(/backend secret/)).toBeNull();
  });

  it("clears dirty actions when edits return to their initial values", async () => {
    render(() => <SettingsPage />);
    const appName = screen.getAllByTestId("input")[0];
    fireEvent.input(appName, { target: { value: "Changed" } });
    expect(
      screen.getByRole("button", { name: "Save changes" }),
    ).toBeInTheDocument();
    fireEvent.input(appName, { target: { value: "Trailbase" } });
    await waitFor(() => {
      expect(screen.queryByRole("button", { name: "Save changes" })).toBeNull();
      expect(screen.queryByRole("button", { name: "Reset" })).toBeNull();
    });
    fireEvent.click(screen.getByText("Email"));
    expect(state.navigate).toHaveBeenLastCalledWith("/settings/email");
  });

  it("resets edited values and clears dirty actions", async () => {
    render(() => <SettingsPage />);
    const appName = screen.getAllByTestId("input")[0];
    fireEvent.input(appName, { target: { value: "Changed" } });
    fireEvent.click(screen.getByRole("button", { name: "Reset" }));
    expect(appName).toHaveValue("Trailbase");
    await waitFor(() => {
      expect(screen.queryByRole("button", { name: "Save changes" })).toBeNull();
      expect(screen.queryByRole("button", { name: "Reset" })).toBeNull();
    });
    fireEvent.click(screen.getByText("Email"));
    expect(state.navigate).toHaveBeenLastCalledWith("/settings/email");
  });

  it("preserves edits made while a save is in flight", async () => {
    let finishSave!: () => void;
    state.setConfig.mockReturnValueOnce(
      new Promise<void>((resolve) => {
        finishSave = resolve;
      }),
    );
    render(() => <SettingsPage />);
    const appName = screen.getAllByTestId("input")[0];
    fireEvent.input(appName, { target: { value: "Changed" } });
    fireEvent.click(screen.getByRole("button", { name: "Save changes" }));
    await waitFor(() => expect(state.setConfig).toHaveBeenCalledOnce());
    fireEvent.input(appName, { target: { value: "Changed while saving" } });
    finishSave();

    await waitFor(() => expect(state.showToast).toHaveBeenCalledOnce());
    expect(appName).toHaveValue("Changed while saving");
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "Save changes" }),
      ).toBeInTheDocument(),
    );
    expect(screen.getByRole("button", { name: "Reset" })).toBeInTheDocument();
  });

  it("does not report a pending save after unmount", async () => {
    let finishSave!: () => void;
    state.setConfig.mockReturnValueOnce(
      new Promise<void>((resolve) => {
        finishSave = resolve;
      }),
    );
    const dom = render(() => <SettingsPage />);
    fireEvent.input(screen.getAllByTestId("input")[0], {
      target: { value: "Changed" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save changes" }));
    dom.unmount();
    finishSave();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(state.showToast).not.toHaveBeenCalled();
  });

  it("rebases clean incoming config updates", async () => {
    render(() => <SettingsPage />);
    state.updateConfigQuery!({
      ...generalConfig(),
      server: { ...generalConfig().server, applicationName: "Remote" },
    });
    await waitFor(() =>
      expect(screen.getAllByTestId("input")[0]).toHaveValue("Remote"),
    );
    expect(screen.queryByRole("button", { name: "Save changes" })).toBeNull();
  });

  it("preserves dirty edits across incoming config and saves against the latest base", async () => {
    render(() => <SettingsPage />);
    const appName = screen.getAllByTestId("input")[0];
    fireEvent.input(appName, { target: { value: "Local edit" } });
    state.updateConfigQuery!({
      ...generalConfig(),
      email: { smtpHost: "latest-base" },
      server: { ...generalConfig().server, applicationName: "Remote" },
    });
    await waitFor(() => expect(appName).toHaveValue("Local edit"));
    fireEvent.click(screen.getByRole("button", { name: "Save changes" }));
    await waitFor(() => expect(state.setConfig).toHaveBeenCalledOnce());
    expect(state.setConfig.mock.calls[0][0].config.email.smtpHost).toBe(
      "latest-base",
    );
  });

  it("keeps edits and actions after failed save, and clears them after success", async () => {
    state.setConfig.mockRejectedValueOnce(new Error("backend secret"));
    render(() => <SettingsPage />);
    const appName = screen.getAllByTestId("input")[0];
    fireEvent.input(appName, { target: { value: "Changed" } });
    fireEvent.click(screen.getByRole("button", { name: "Save changes" }));
    await waitFor(() =>
      expect(screen.getByText("Unable to save settings")).toBeInTheDocument(),
    );
    expect(appName).toHaveValue("Changed");
    expect(screen.getByRole("button", { name: "Reset" })).toBeInTheDocument();
    expect(screen.queryByText("backend secret")).toBeNull();
    let finishSave!: () => void;
    state.setConfig.mockReturnValueOnce(
      new Promise<void>((resolve) => {
        finishSave = resolve;
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: "Save changes" }));
    expect(state.showToast).not.toHaveBeenCalled();
    finishSave();
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: "Save changes" })).toBeNull(),
    );
    expect(state.setConfig).toHaveBeenCalled();
    await waitFor(() =>
      expect(state.showToast).toHaveBeenCalledWith({
        title: "submitted",
        variant: "success",
      }),
    );

    fireEvent.input(appName, { target: { value: "Changed Again" } });
    expect(
      screen.getByRole("button", { name: "Save changes" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Reset" })).toBeInTheDocument();
    fireEvent.click(screen.getByText("Email"));
    expect(state.navigate).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    fireEvent.click(screen.getByRole("button", { name: "Reset" }));
    expect(appName).toHaveValue("Changed");
    await waitFor(() => {
      expect(screen.queryByRole("button", { name: "Save changes" })).toBeNull();
      expect(screen.queryByRole("button", { name: "Reset" })).toBeNull();
    });
  });
});
