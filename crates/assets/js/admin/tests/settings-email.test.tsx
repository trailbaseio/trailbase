import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const state = vi.hoisted(() => ({
  config: undefined as any,
  configLoading: false,
  configError: false,
  updateConfigQuery: undefined as undefined | ((config: any) => void),
  setConfig: vi.fn(),
  adminFetch: vi.fn(),
  showToast: vi.fn(),
  userEmail: "admin@example.com",
  queryClient: {},
}));

vi.mock("@tanstack/solid-query", () => ({
  useQueryClient: () => state.queryClient,
}));
vi.mock("@nanostores/solid", () => ({
  useStore: () => () => ({ email: state.userEmail }),
}));
vi.mock("@/lib/client", () => ({ $user: {} }));
vi.mock("@/lib/api/config", async () => {
  const { createSignal } =
    await vi.importActual<typeof import("solid-js")>("solid-js");
  return {
    createConfigQuery: () => {
      const [data, setData] = createSignal(
        state.config === undefined ? undefined : { config: state.config },
      );
      state.updateConfigQuery = (config) => setData({ config });
      return {
        get data() {
          return data();
        },
        get isLoading() {
          return state.configLoading;
        },
        get isError() {
          return state.configError;
        },
        error: new Error("raw config detail"),
      };
    },
    setConfig: state.setConfig,
  };
});
vi.mock("@/lib/fetch", () => ({ adminFetch: state.adminFetch }));
vi.mock("@/components/ui/toast", () => ({ showToast: state.showToast }));
vi.mock("@/components/ui/dialog", async () => {
  const Solid = await vi.importActual<typeof import("solid-js")>("solid-js");
  const DialogContext = Solid.createContext<{
    open: () => boolean;
    setOpen: (open: boolean) => void;
  }>();
  return {
    Dialog: (props: any) => (
      <DialogContext.Provider
        value={{
          open: () => props.open,
          setOpen: (open) => props.onOpenChange(open),
        }}
      >
        {props.children}
      </DialogContext.Provider>
    ),
    DialogContent: (props: any) => {
      const dialog = Solid.useContext(DialogContext)!;
      return (
        <Solid.Show when={dialog.open()}>
          <div role="dialog">{props.children}</div>
        </Solid.Show>
      );
    },
    DialogTitle: (props: any) => <h2>{props.children}</h2>,
    DialogFooter: (props: any) => <div>{props.children}</div>,
  };
});
vi.mock("@/components/ui/accordion", async () => {
  const Solid = await vi.importActual<typeof import("solid-js")>("solid-js");
  const AccordionContext = Solid.createContext<{
    open: () => string[];
    toggle: (value: string) => void;
  }>();
  const ItemContext = Solid.createContext<string>();
  return {
    Accordion: (props: any) => {
      const [open, setOpen] = Solid.createSignal<string[]>([]);
      return (
        <AccordionContext.Provider
          value={{
            open,
            toggle: (value) =>
              setOpen((current) =>
                current.includes(value)
                  ? current.filter((item) => item !== value)
                  : props.multiple
                    ? [...current, value]
                    : [value],
              ),
          }}
        >
          <div>{props.children}</div>
        </AccordionContext.Provider>
      );
    },
    AccordionItem: (props: any) => (
      <ItemContext.Provider value={props.value}>
        <section>{props.children}</section>
      </ItemContext.Provider>
    ),
    AccordionTrigger: (props: any) => {
      const accordion = Solid.useContext(AccordionContext)!;
      const item = Solid.useContext(ItemContext)!;
      return (
        <button
          type="button"
          aria-expanded={accordion.open().includes(item)}
          onClick={() => accordion.toggle(item)}
        >
          {props.children}
        </button>
      );
    },
    AccordionContent: (props: any) => {
      const accordion = Solid.useContext(AccordionContext)!;
      const item = Solid.useContext(ItemContext)!;
      return (
        <Solid.Show when={accordion.open().includes(item)}>
          <div>{props.children}</div>
        </Solid.Show>
      );
    },
  };
});

import { EmailSettings } from "@/components/settings/EmailSettings";

const initialConfig = () => ({
  email: {
    smtpHost: "smtp.initial.test",
    smtpPort: 587,
    smtpUsername: "initial-user",
    smtpPassword: "initial-password",
    senderName: "Initial Sender",
    senderAddress: "sender@initial.test",
    userVerificationTemplate: {
      subject: "Initial verification subject",
      body: "Initial verification body",
    },
  },
  server: {
    applicationName: "Initial App",
    siteUrl: "https://initial.test",
  },
  databases: [{ name: "initial-db" }],
});

function deferred<T = void>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function renderSettings() {
  const setDirty = vi.fn();
  const postSubmit = vi.fn();
  const dom = render(() => (
    <EmailSettings setDirty={setDirty} postSubmit={postSubmit} />
  ));
  return { ...dom, setDirty, postSubmit };
}

function input(name: string) {
  return screen.getByLabelText(name) as HTMLInputElement;
}

async function save() {
  fireEvent.click(screen.getByRole("button", { name: "Save changes" }));
  await waitFor(() => expect(state.setConfig).toHaveBeenCalled());
}

beforeEach(() => {
  state.config = initialConfig();
  state.configLoading = false;
  state.configError = false;
  state.updateConfigQuery = undefined;
  state.setConfig.mockReset();
  state.setConfig.mockResolvedValue(undefined);
  state.adminFetch.mockReset();
  state.showToast.mockReset();
  state.userEmail = "admin@example.com";
});

afterEach(cleanup);

describe("Email settings loading", () => {
  it("shows loading and a generic config error without raw detail", () => {
    state.configLoading = true;
    renderSettings();
    expect(screen.getByRole("status")).toHaveTextContent("Loading settings...");

    cleanup();
    state.configLoading = false;
    state.configError = true;
    renderSettings();
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Unable to load settings",
    );
    expect(screen.queryByText(/raw config detail/)).toBeNull();
  });

  it("renders an editable empty form when loaded Config has no email", () => {
    state.config = { server: { applicationName: "TrailBase" } };
    renderSettings();
    expect(input("Host")).toHaveValue("");
    expect(input("Sender Name")).toHaveValue("");
    fireEvent.change(input("Host"), { target: { value: "smtp.new.test" } });
    expect(input("Host")).toHaveValue("smtp.new.test");
  });
});

describe("Email settings form lifecycle", () => {
  it("shows all sections, persistent placeholder guidance, and detailed template parameters", () => {
    renderSettings();
    for (const section of ["SMTP", "Sender", "Templates"])
      expect(
        screen.getByRole("heading", { name: section }),
      ).toBeInTheDocument();
    expect(
      screen.getByText(/template placeholders use.*\{\{ PARAMETER \}\}/i),
    ).toBeVisible();
    expect(screen.queryByText(/VERIFICATION_URL/)).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Email Verification" }));
    expect(screen.getByText(/VERIFICATION_URL/)).toBeVisible();
  });

  it("keeps the SMTP password secret and never exposes a failed-save secret", async () => {
    const secret = "sentinel-email-secret";
    state.config = {
      ...initialConfig(),
      email: { ...initialConfig().email, smtpPassword: secret },
    };
    state.setConfig.mockRejectedValueOnce(new Error(secret));
    const consoleSpies = ["error", "warn", "log"].map((method) =>
      vi.spyOn(console, method as "error").mockImplementation(() => undefined),
    );
    renderSettings();

    expect(input("Password")).toHaveAttribute("type", "password");
    expect(document.body.textContent).not.toContain(secret);
    fireEvent.change(input("Host"), { target: { value: "smtp.failed.test" } });
    await save();
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(
        "Unable to save settings",
      ),
    );
    expect(screen.getByRole("alert")).not.toHaveTextContent(secret);
    expect(JSON.stringify(state.showToast.mock.calls)).not.toContain(secret);
    expect(
      consoleSpies
        .flatMap((spy) => spy.mock.calls.flat())
        .map(String)
        .join("\n"),
    ).not.toContain(secret);
    consoleSpies.forEach((spy) => spy.mockRestore());
  });

  it("clears dirty state on revert and resets dirty edits", async () => {
    renderSettings();
    expect(screen.queryByRole("button", { name: "Save changes" })).toBeNull();

    fireEvent.change(input("Host"), { target: { value: "smtp.changed.test" } });
    expect(
      screen.getByRole("button", { name: "Save changes" }),
    ).toBeInTheDocument();
    fireEvent.change(input("Host"), {
      target: { value: "smtp.initial.test" },
    });
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: "Save changes" })).toBeNull(),
    );

    fireEvent.change(input("Sender Name"), { target: { value: "Local" } });
    fireEvent.click(screen.getByRole("button", { name: "Reset" }));
    expect(input("Sender Name")).toHaveValue("Initial Sender");
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: "Reset" })).toBeNull(),
    );
  });

  it("retains edits and actions after a generic failed save", async () => {
    state.setConfig.mockRejectedValueOnce(new Error("private backend detail"));
    renderSettings();
    fireEvent.change(input("Host"), { target: { value: "smtp.failed.test" } });
    await save();

    await waitFor(() =>
      expect(screen.getByText("Unable to save settings")).toBeInTheDocument(),
    );
    expect(input("Host")).toHaveValue("smtp.failed.test");
    expect(screen.getByRole("button", { name: "Reset" })).toBeInTheDocument();
    expect(screen.queryByText(/private backend detail/)).toBeNull();
  });

  it("awaits save success, rebases, and tracks a post-success edit", async () => {
    const request = deferred<void>();
    state.setConfig.mockReturnValueOnce(request.promise);
    const { postSubmit } = renderSettings();
    fireEvent.change(input("Host"), { target: { value: "smtp.saved.test" } });
    await save();
    expect(postSubmit).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Saving…" })).toBeDisabled();

    request.resolve();
    await waitFor(() => expect(postSubmit).toHaveBeenCalledWith(false));
    expect(screen.queryByRole("button", { name: "Save changes" })).toBeNull();

    fireEvent.change(input("Host"), {
      target: { value: "smtp.after-success.test" },
    });
    expect(
      screen.getByRole("button", { name: "Save changes" }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Reset" }));
    expect(input("Host")).toHaveValue("smtp.saved.test");
  });

  it("preserves a newer edit made while save is pending", async () => {
    const request = deferred<void>();
    state.setConfig.mockReturnValueOnce(request.promise);
    const { postSubmit } = renderSettings();
    fireEvent.change(input("Host"), {
      target: { value: "smtp.submitted.test" },
    });
    await save();
    fireEvent.change(input("Host"), { target: { value: "smtp.newer.test" } });

    request.resolve();
    await waitFor(() => expect(postSubmit).toHaveBeenCalledWith(true));
    expect(input("Host")).toHaveValue("smtp.newer.test");
    expect(
      screen.getByRole("button", { name: "Save changes" }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Reset" }));
    expect(input("Host")).toHaveValue("smtp.submitted.test");
  });

  it("suppresses pending save callbacks after unmount", async () => {
    const request = deferred<void>();
    state.setConfig.mockReturnValueOnce(request.promise);
    const { postSubmit, unmount } = renderSettings();
    fireEvent.change(input("Host"), { target: { value: "smtp.saved.test" } });
    await save();
    unmount();
    request.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(postSubmit).not.toHaveBeenCalled();
  });

  it("rebases a clean form to incoming query data", async () => {
    renderSettings();
    state.updateConfigQuery!({
      ...initialConfig(),
      email: { ...initialConfig().email, smtpHost: "smtp.remote.test" },
    });
    await waitFor(() => expect(input("Host")).toHaveValue("smtp.remote.test"));
    expect(screen.queryByRole("button", { name: "Save changes" })).toBeNull();
  });

  it("preserves a dirty edit across incoming query data", async () => {
    renderSettings();
    fireEvent.change(input("Host"), { target: { value: "smtp.local.test" } });
    state.updateConfigQuery!({
      ...initialConfig(),
      email: {
        ...initialConfig().email,
        smtpHost: "smtp.remote.test",
        senderName: "Remote Sender",
      },
    });
    await waitFor(() => expect(input("Host")).toHaveValue("smtp.local.test"));
    expect(input("Sender Name")).toHaveValue("Initial Sender");
    expect(
      screen.getByRole("button", { name: "Save changes" }),
    ).toBeInTheDocument();
  });

  it("three-way merges a local scalar onto latest Email and full Config", async () => {
    renderSettings();
    fireEvent.change(input("Host"), { target: { value: "smtp.local.test" } });
    state.updateConfigQuery!({
      ...initialConfig(),
      email: {
        ...initialConfig().email,
        senderName: "Remote Sender",
      },
      server: {
        applicationName: "Remote App",
        siteUrl: "https://remote.test",
        enableRecordTransactions: true,
      },
      databases: [{ name: "remote-db" }],
    });
    await save();

    const saved = state.setConfig.mock.calls[0][0].config;
    expect(saved.email.smtpHost).toBe("smtp.local.test");
    expect(saved.email.senderName).toBe("Remote Sender");
    expect(saved.server).toMatchObject({
      applicationName: "Remote App",
      siteUrl: "https://remote.test",
      enableRecordTransactions: true,
    });
    expect(saved.databases).toEqual([
      expect.objectContaining({ name: "remote-db" }),
    ]);
  });

  it("merges template subject and body as independent leaves", async () => {
    renderSettings();
    fireEvent.click(screen.getByRole("button", { name: "Email Verification" }));
    fireEvent.change(input("Subject"), { target: { value: "Local subject" } });
    state.updateConfigQuery!({
      ...initialConfig(),
      email: {
        ...initialConfig().email,
        userVerificationTemplate: {
          subject: "Initial verification subject",
          body: "Remote body",
        },
      },
    });
    await save();

    expect(
      state.setConfig.mock.calls[0][0].config.email.userVerificationTemplate,
    ).toEqual({ subject: "Local subject", body: "Remote body" });
  });

  it("keeps an incoming query refresh as the final baseline during awaited save", async () => {
    const request = deferred<void>();
    state.setConfig.mockReturnValueOnce(request.promise);
    const { postSubmit } = renderSettings();
    fireEvent.change(input("Host"), {
      target: { value: "smtp.submitted.test" },
    });
    await save();

    state.updateConfigQuery!({
      ...initialConfig(),
      email: {
        ...initialConfig().email,
        smtpHost: "smtp.remote-stale.test",
        senderName: "Refreshed Sender",
      },
    });
    request.resolve();

    await waitFor(() => expect(postSubmit).toHaveBeenCalledWith(false));
    expect(input("Host")).toHaveValue("smtp.submitted.test");
    expect(input("Sender Name")).toHaveValue("Refreshed Sender");
    fireEvent.change(input("Sender Name"), { target: { value: "Another" } });
    fireEvent.click(screen.getByRole("button", { name: "Reset" }));
    expect(input("Host")).toHaveValue("smtp.submitted.test");
    expect(input("Sender Name")).toHaveValue("Refreshed Sender");
  });
});

describe("Test Email dialog", () => {
  it("posts the exact request, blocks duplicate actions, and closes only after success", async () => {
    const request = deferred<Response>();
    state.adminFetch.mockReturnValueOnce(request.promise);
    renderSettings();
    fireEvent.click(screen.getByRole("button", { name: "Send Test Email" }));
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(input("Email")).toHaveValue("admin@example.com");
    fireEvent.change(input("Email"), {
      target: { value: "target@example.com" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));

    await waitFor(() => expect(state.adminFetch).toHaveBeenCalledOnce());
    expect(state.adminFetch).toHaveBeenCalledWith("/email/test", {
      method: "POST",
      body: JSON.stringify({ email_address: "target@example.com" }),
      throwOnError: true,
    });
    expect(screen.getByRole("button", { name: "Sending…" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Close" })).toBeDisabled();
    expect(screen.getByRole("status")).toHaveTextContent("Sending…");
    fireEvent.click(screen.getByRole("button", { name: "Sending…" }));
    expect(state.adminFetch).toHaveBeenCalledOnce();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(state.showToast).not.toHaveBeenCalled();

    request.resolve(new Response());
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(state.showToast).toHaveBeenCalledWith({
      title: "Sent to target@example.com",
      variant: "success",
    });
  });

  it("stays open with a generic error after failure and allows retry", async () => {
    state.adminFetch
      .mockRejectedValueOnce(new Error("sentinel-network-secret"))
      .mockResolvedValueOnce(new Response());
    renderSettings();
    fireEvent.click(screen.getByRole("button", { name: "Send Test Email" }));
    fireEvent.click(screen.getByRole("button", { name: "Send" }));

    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(
        "Unable to send test email",
      ),
    );
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(screen.getByRole("alert")).not.toHaveTextContent(
      "sentinel-network-secret",
    );
    expect(screen.getByRole("button", { name: "Send" })).toBeEnabled();

    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() => expect(state.adminFetch).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("does not close or toast when unmounted during a pending request", async () => {
    const request = deferred<Response>();
    state.adminFetch.mockReturnValueOnce(request.promise);
    const { unmount } = renderSettings();
    fireEvent.click(screen.getByRole("button", { name: "Send Test Email" }));
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() => expect(state.adminFetch).toHaveBeenCalledOnce());
    unmount();

    request.resolve(new Response());
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(state.showToast).not.toHaveBeenCalled();
  });
});
