import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import * as Solid from "solid-js";

const state = vi.hoisted(() => ({
  params: { group: "host" } as { group?: string },
  mobile: false,
  navigate: vi.fn(),
  invalidate: vi.fn(),
  childProps: undefined as any,
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
}));
vi.mock("@/lib/signals", () => ({ createIsMobile: () => () => state.mobile }));
vi.mock("@/lib/api/config", () => ({
  createConfigQuery: () => ({
    data: { config: undefined },
    isLoading: false,
    isError: false,
  }),
  setConfig: vi.fn(),
  invalidateConfig: state.invalidate,
}));
vi.mock("@/lib/api/info", () => ({
  createSystemInfoQuery: () => ({
    isLoading: false,
    isError: false,
    isSuccess: false,
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
    expect(screen.getByRole("button", { name: "Save changes" })).toBeDisabled();
  });
});
