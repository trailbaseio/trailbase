import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const state = vi.hoisted(() => ({
  config: undefined as any,
  configLoading: false,
  configError: false,
  providers: undefined as any,
  providersLoading: false,
  providersError: false,
  updateConfigQuery: undefined as undefined | ((config: any) => void),
  updateProvidersQuery: undefined as undefined | ((providers: any) => void),
  setConfig: vi.fn(),
  adminFetch: vi.fn(),
  showSaveFileDialog: vi.fn(),
  copyToClipboard: vi.fn(),
  queryClient: {},
}));

vi.mock("@tanstack/solid-query", async () => {
  const Solid = await vi.importActual<typeof import("solid-js")>("solid-js");
  return {
    useQueryClient: () => state.queryClient,
    useQuery: () => {
      const [data, setData] = Solid.createSignal(state.providers);
      state.updateProvidersQuery = setData;
      return {
        get data() {
          return data();
        },
        get isLoading() {
          return state.providersLoading;
        },
        get error() {
          return state.providersError
            ? new Error("raw provider detail")
            : undefined;
        },
      };
    },
  };
});

vi.mock("@/lib/api/config", async () => {
  const Solid = await vi.importActual<typeof import("solid-js")>("solid-js");
  return {
    createConfigQuery: () => {
      const [data, setData] = Solid.createSignal(
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
vi.mock("@/lib/utils", async () => {
  const actual =
    await vi.importActual<typeof import("@/lib/utils")>("@/lib/utils");
  return {
    ...actual,
    showSaveFileDialog: state.showSaveFileDialog,
    copyToClipboard: state.copyToClipboard,
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

vi.mock("@/components/ui/tooltip", () => ({
  Tooltip: (props: any) => <span>{props.children}</span>,
  TooltipTrigger: (props: any) => (
    <span onClick={props.onClick}>{props.children}</span>
  ),
  TooltipContent: (props: any) => <span>{props.children}</span>,
}));

import {
  AuthConfig,
  Config,
  OAuthProviderConfig,
  OAuthProviderId,
} from "@proto/config";
import {
  AuthSettings,
  configToProxy,
  proxyToConfig,
  mergeAuthLeaves,
} from "@/components/settings/AuthSettings";

const provider = {
  id: OAuthProviderId.GITHUB,
  name: "github",
  display_name: "GitHub",
};

const proxyFor = (auth: AuthConfig, providers = [provider]) =>
  configToProxy(providers, auth);

describe("authentication settings proxy", () => {
  it("round-trips complete providers and OIDC fields", () => {
    const auth = AuthConfig.fromPartial({
      authTokenTtlSec: 12n,
      customUriSchemes: ["my-app"],
      oauthProviders: {
        github: OAuthProviderConfig.fromPartial({
          providerId: provider.id,
          clientId: "id",
          clientSecret: "secret",
        }),
      },
    });
    const proxy = configToProxy([provider], auth);
    const result = proxyToConfig(proxy);
    expect(result.authTokenTtlSec).toBe(12n);
    expect(result.customUriSchemes).toEqual(["my-app"]);
    expect(result.oauthProviders.github?.clientSecret).toBe("secret");
  });

  it("does not submit incomplete credentials", () => {
    const proxy = configToProxy([provider], AuthConfig.create());
    proxy.namedOAuthProviders[0].state = OAuthProviderConfig.fromPartial({
      providerId: provider.id,
      clientId: "only-id",
    });
    expect(proxyToConfig(proxy).oauthProviders).toEqual({});
  });

  it("preserves absent auth as an empty config", () => {
    expect(proxyToConfig(proxyFor(AuthConfig.create()))).toEqual(
      AuthConfig.create(),
    );
  });

  it("detects arrays whose joined strings would collide", () => {
    const base = proxyFor(
      AuthConfig.fromPartial({ customUriSchemes: ["a,b"] }),
    );
    const submitted = proxyFor(
      AuthConfig.fromPartial({ customUriSchemes: ["a", "b"] }),
    );
    const merged = mergeAuthLeaves(submitted, base, AuthConfig.create());
    expect(merged.customUriSchemes).toEqual(["a", "b"]);
  });

  it("merges changed scalar leaves onto a refreshed auth config", () => {
    const base = proxyFor(AuthConfig.fromPartial({ authTokenTtlSec: 1n }));
    const submitted = proxyFor(AuthConfig.fromPartial({ authTokenTtlSec: 2n }));
    const remote = AuthConfig.fromPartial({ refreshTokenTtlSec: 9n });
    const merged = mergeAuthLeaves(submitted, base, remote);
    expect(merged.authTokenTtlSec).toBe(2n);
    expect(merged.refreshTokenTtlSec).toBe(9n);
  });

  it("merges provider leaves independently", () => {
    const base = proxyFor(
      AuthConfig.fromPartial({
        oauthProviders: {
          github: OAuthProviderConfig.fromPartial({
            providerId: provider.id,
            clientId: "old",
            clientSecret: "secret",
          }),
        },
      }),
    );
    const submitted = proxyFor(
      AuthConfig.fromPartial({
        oauthProviders: {
          github: OAuthProviderConfig.fromPartial({
            providerId: provider.id,
            clientId: "new",
            clientSecret: "secret",
          }),
        },
      }),
    );
    const remote = AuthConfig.fromPartial({
      oauthProviders: {
        github: OAuthProviderConfig.fromPartial({
          providerId: provider.id,
          clientId: "old",
          clientSecret: "secret",
          displayName: "Remote",
        }),
      },
    });
    const merged = mergeAuthLeaves(submitted, base, remote);
    expect(merged.oauthProviders.github?.clientId).toBe("new");
    expect(merged.oauthProviders.github?.displayName).toBe("Remote");
  });

  it("changes provider scopes without overwriting refreshed URLs", () => {
    const make = (scopes: string[], authUrl = "remote-auth") =>
      OAuthProviderConfig.fromPartial({
        providerId: provider.id,
        clientId: "id",
        clientSecret: "secret",
        scopes,
        authUrl,
      });
    const base = proxyFor(
      AuthConfig.fromPartial({
        oauthProviders: { github: make(["old"], "old-auth") },
      }),
    );
    const submitted = proxyFor(
      AuthConfig.fromPartial({
        oauthProviders: { github: make(["new"], "old-auth") },
      }),
    );
    const merged = mergeAuthLeaves(
      submitted,
      base,
      AuthConfig.fromPartial({
        oauthProviders: { github: make(["old"], "remote-auth") },
      }),
    );
    expect(merged.oauthProviders.github?.scopes).toEqual(["new"]);
    expect(merged.oauthProviders.github?.authUrl).toBe("remote-auth");
  });

  it("removes a provider intentionally made incomplete", () => {
    const base = proxyFor(
      AuthConfig.fromPartial({
        oauthProviders: {
          github: OAuthProviderConfig.fromPartial({
            providerId: provider.id,
            clientId: "id",
            clientSecret: "secret",
          }),
        },
      }),
    );
    const submitted = proxyFor(AuthConfig.create());
    submitted.namedOAuthProviders[0].state = OAuthProviderConfig.fromPartial({
      providerId: provider.id,
      clientId: "id",
    });
    const merged = mergeAuthLeaves(
      submitted,
      base,
      AuthConfig.fromPartial({
        oauthProviders: {
          github: OAuthProviderConfig.fromPartial({
            providerId: provider.id,
            clientId: "id",
            clientSecret: "secret",
          }),
        },
      }),
    );
    expect(merged.oauthProviders).toEqual({});
  });

  it("keeps an untouched remote provider", () => {
    const base = proxyFor(AuthConfig.create());
    const submitted = proxyFor(AuthConfig.create());
    const remoteProvider = OAuthProviderConfig.fromPartial({
      providerId: provider.id,
      clientId: "id",
      clientSecret: "secret",
    });
    const merged = mergeAuthLeaves(
      submitted,
      base,
      AuthConfig.fromPartial({ oauthProviders: { github: remoteProvider } }),
    );
    expect(
      OAuthProviderConfig.encode(merged.oauthProviders.github!).finish(),
    ).toEqual(OAuthProviderConfig.encode(remoteProvider).finish());
  });

  it("round-trips OIDC endpoint and scope leaves", () => {
    const oidc = {
      id: OAuthProviderId.OIDC0,
      name: "oidc0",
      display_name: "OIDC",
    };
    const auth = AuthConfig.fromPartial({
      oauthProviders: {
        oidc0: OAuthProviderConfig.fromPartial({
          providerId: oidc.id,
          clientId: "id",
          clientSecret: "secret",
          authUrl: "a",
          tokenUrl: "t",
          userApiUrl: "u",
          scopes: ["openid", "email"],
        }),
      },
    });
    const roundTrip = proxyToConfig(proxyFor(auth, [oidc]));
    expect(roundTrip.oauthProviders.oidc0?.authUrl).toBe("a");
    expect(roundTrip.oauthProviders.oidc0?.scopes).toEqual(["openid", "email"]);
  });

  it("keeps redirect allowlist order and values", () => {
    const auth = AuthConfig.fromPartial({ redirectUriAllowlist: ["a,b", "c"] });
    expect(proxyToConfig(proxyFor(auth)).redirectUriAllowlist).toEqual([
      "a,b",
      "c",
    ]);
  });

  it("removes a provider when the saved provider is explicitly cleared", () => {
    const configured = OAuthProviderConfig.fromPartial({
      providerId: provider.id,
      clientId: "id",
      clientSecret: "secret",
    });
    const base = proxyFor(
      AuthConfig.fromPartial({ oauthProviders: { github: configured } }),
    );
    const submitted = proxyFor(AuthConfig.create());
    const merged = mergeAuthLeaves(
      submitted,
      base,
      AuthConfig.fromPartial({ oauthProviders: { github: configured } }),
    );
    expect(merged.oauthProviders.github).toBeUndefined();
  });

  it("preserves unrelated auth lists while changing one list", () => {
    const base = proxyFor(
      AuthConfig.fromPartial({
        customUriSchemes: ["one"],
        redirectUriAllowlist: ["keep"],
      }),
    );
    const submitted = proxyFor(
      AuthConfig.fromPartial({
        customUriSchemes: ["two"],
        redirectUriAllowlist: ["keep"],
      }),
    );
    const merged = mergeAuthLeaves(submitted, base, AuthConfig.create());
    expect(merged.customUriSchemes).toEqual(["two"]);
    expect(merged.redirectUriAllowlist).toEqual([]);
  });

  it("retains refreshed provider credentials when local endpoint is unchanged", () => {
    const make = (clientSecret: string) =>
      OAuthProviderConfig.fromPartial({
        providerId: provider.id,
        clientId: "id",
        clientSecret,
        authUrl: "url",
      });
    const base = proxyFor(
      AuthConfig.fromPartial({ oauthProviders: { github: make("old") } }),
    );
    const submitted = proxyFor(
      AuthConfig.fromPartial({ oauthProviders: { github: make("old") } }),
    );
    const merged = mergeAuthLeaves(
      submitted,
      base,
      AuthConfig.fromPartial({ oauthProviders: { github: make("refreshed") } }),
    );
    expect(merged.oauthProviders.github?.clientSecret).toBe("refreshed");
  });

  it("preserves provider id and secret during a display name refresh", () => {
    const make = (displayName: string) =>
      OAuthProviderConfig.fromPartial({
        providerId: provider.id,
        clientId: "id",
        clientSecret: "secret",
        displayName,
      });
    const base = proxyFor(
      AuthConfig.fromPartial({ oauthProviders: { github: make("old") } }),
    );
    const submitted = proxyFor(
      AuthConfig.fromPartial({ oauthProviders: { github: make("new") } }),
    );
    const merged = mergeAuthLeaves(
      submitted,
      base,
      AuthConfig.fromPartial({ oauthProviders: { github: make("remote") } }),
    );
    expect(merged.oauthProviders.github?.clientId).toBe("id");
    expect(merged.oauthProviders.github?.displayName).toBe("new");
  });
});

const oidcProvider = {
  id: OAuthProviderId.OIDC0,
  name: "oidc0",
  display_name: "OIDC",
};
const discordProvider = {
  id: OAuthProviderId.DISCORD,
  name: "discord",
  display_name: "Discord",
};
const providers = { providers: [oidcProvider, provider, discordProvider] };

const providerConfig = (
  providerId: OAuthProviderId,
  values: Partial<OAuthProviderConfig> = {},
) =>
  OAuthProviderConfig.fromPartial({
    providerId,
    clientId: "initial-id",
    clientSecret: "sentinel-provider-secret",
    ...values,
  });

function initialConfig(siteUrl: string | undefined = "https://initial.test") {
  return Config.fromPartial({
    server: {
      applicationName: "Initial App",
      siteUrl,
      enableRecordTransactions: false,
    },
    auth: AuthConfig.fromPartial({
      authTokenTtlSec: 3600n,
      refreshTokenTtlSec: 7200n,
      customUriSchemes: ["initial-app"],
      oauthProviders: {
        oidc0: providerConfig(OAuthProviderId.OIDC0, {
          authUrl: "https://oidc.initial/auth",
          tokenUrl: "https://oidc.initial/token",
          userApiUrl: "https://oidc.initial/user",
          scopes: ["openid", "email"],
        }),
        github: providerConfig(OAuthProviderId.GITHUB, {
          displayName: "Initial GitHub",
          authUrl: "https://github.initial/auth",
          scopes: ["read:user"],
        }),
      },
    }),
    databases: [{ name: "initial-db" }],
  });
}

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
    <AuthSettings setDirty={setDirty} postSubmit={postSubmit} />
  ));
  return { ...dom, setDirty, postSubmit };
}

function openProvider(name: string) {
  const trigger = screen.getByRole("button", { name: new RegExp(name, "i") });
  fireEvent.click(trigger);
  return trigger.closest("section")!;
}

function providerInput(section: HTMLElement, name: string) {
  return within(section).getByLabelText(name) as HTMLInputElement;
}

async function save() {
  const expectedCalls = state.setConfig.mock.calls.length + 1;
  fireEvent.click(screen.getByRole("button", { name: "Save changes" }));
  await waitFor(() =>
    expect(state.setConfig.mock.calls.length).toBe(expectedCalls),
  );
}

beforeEach(() => {
  state.config = initialConfig();
  state.configLoading = false;
  state.configError = false;
  state.providers = providers;
  state.providersLoading = false;
  state.providersError = false;
  state.updateConfigQuery = undefined;
  state.updateProvidersQuery = undefined;
  state.setConfig.mockReset();
  state.setConfig.mockResolvedValue(undefined);
  state.adminFetch.mockReset();
  state.showSaveFileDialog.mockReset();
  state.copyToClipboard.mockReset();
});

afterEach(cleanup);

describe("AuthSettings UI and lifecycle", () => {
  it("loads providers and config, hides raw errors, and renders absent auth", () => {
    state.providersLoading = true;
    renderSettings();
    expect(screen.getByRole("status")).toHaveTextContent(
      "Loading authentication settings...",
    );

    cleanup();
    state.providersLoading = false;
    state.configLoading = true;
    renderSettings();
    expect(screen.getByRole("status")).toHaveTextContent(
      "Loading authentication settings...",
    );

    cleanup();
    state.configLoading = false;
    state.configError = true;
    renderSettings();
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Unable to load authentication settings",
    );
    expect(
      screen.queryByText(/raw config detail|raw provider detail/),
    ).toBeNull();

    cleanup();
    state.configError = false;
    state.config = Config.fromPartial({
      server: { applicationName: "No Auth App" },
    });
    renderSettings();
    expect(
      screen.getByRole("heading", { name: "Password & User Settings" }),
    ).toBeInTheDocument();
    expect(providerInput(openProvider("GitHub"), "Client Id")).toHaveValue("");
  });

  it("shows provider errors generically without backend details", () => {
    state.providersError = true;
    renderSettings();
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Unable to load authentication settings",
    );
    expect(screen.queryByText(/raw provider detail/)).toBeNull();
  });

  it("shows all five sections and persistent security guidance", () => {
    renderSettings();
    for (const heading of [
      "Password & User Settings",
      "OTP Settings",
      "Token Settings",
      "OAuth Providers",
      "Public Key",
    ]) {
      expect(screen.getByRole("heading", { name: heading })).toBeVisible();
    }
    expect(screen.getByText(/Changing the user identifier/)).toBeVisible();
    expect(screen.getByText(/Enabling OTP sign-in/)).toBeVisible();
    expect(
      screen.getByText(/keep the corresponding private key secret/),
    ).toBeVisible();
  });

  it("uses configured and origin callback URLs and safe provider links", () => {
    renderSettings();
    openProvider("Discord");
    expect(
      screen.getByText(
        "https://initial.test/api/auth/v1/oauth/discord/callback",
      ),
    ).toBeVisible();
    const link = screen.getByRole("link", { name: "Discord" });
    expect(link).toHaveAttribute(
      "href",
      "https://discord.com/developers/applications",
    );
    expect(link).toHaveAttribute("target", "_blank");
    expect(link).toHaveAttribute("rel", "noreferrer noopener");

    cleanup();
    state.config = initialConfig();
    state.config.server!.siteUrl = undefined;
    renderSettings();
    openProvider("Discord");
    expect(
      screen.getByText(
        `${window.location.origin}/api/auth/v1/oauth/discord/callback`,
      ),
    ).toBeVisible();
  });

  it("keeps provider secrets masked and failed-save details private", async () => {
    const secret = "sentinel-provider-secret";
    state.setConfig.mockRejectedValueOnce(new Error(secret));
    const consoleSpies = ["error", "warn", "log"].map((method) =>
      vi.spyOn(console, method as "error").mockImplementation(() => undefined),
    );
    renderSettings();
    const github = openProvider("GitHub");
    expect(providerInput(github, "Client Secret")).toHaveAttribute(
      "type",
      "password",
    );
    expect(document.body.textContent).not.toContain(secret);

    fireEvent.change(providerInput(github, "Client Id"), {
      target: { value: "failed-id" },
    });
    await save();
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(
        "Unable to save settings",
      ),
    );
    expect(screen.getByRole("alert")).not.toHaveTextContent(secret);
    expect(providerInput(github, "Client Id")).toHaveValue("failed-id");
    expect(screen.getByRole("button", { name: "Reset" })).toBeVisible();
    expect(
      consoleSpies.flatMap((spy) => spy.mock.calls.flat()).join("\n"),
    ).not.toContain(secret);
    consoleSpies.forEach((spy) => spy.mockRestore());
  });

  it("tracks edit-back and global/provider reset and remove actions", async () => {
    renderSettings();
    const github = openProvider("GitHub");
    const clientId = providerInput(github, "Client Id");
    expect(screen.queryByRole("button", { name: "Save changes" })).toBeNull();

    fireEvent.change(clientId, { target: { value: "local-id" } });
    expect(screen.getByRole("button", { name: "Save changes" })).toBeVisible();
    fireEvent.change(clientId, { target: { value: "initial-id" } });
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: "Save changes" })).toBeNull(),
    );

    fireEvent.change(clientId, { target: { value: "local-id" } });
    fireEvent.click(
      within(github).getByRole("button", { name: "Reset GitHub" }),
    );
    expect(clientId).toHaveValue("initial-id");
    fireEvent.click(
      within(github).getByRole("button", { name: "Remove GitHub" }),
    );
    expect(clientId).toHaveValue("");
    fireEvent.click(screen.getByRole("button", { name: "Reset" }));
    expect(clientId).toHaveValue("initial-id");
  });

  it("restores all OIDC credentials, endpoints, and scopes", () => {
    renderSettings();
    const oidc = openProvider("OIDC");
    const fields = [
      ["Client Id", "changed-id", "initial-id"],
      ["Client Secret", "changed-secret", "sentinel-provider-secret"],
      ["Auth URL", "https://changed/auth", "https://oidc.initial/auth"],
      ["Token URL", "https://changed/token", "https://oidc.initial/token"],
      ["User API URL", "https://changed/user", "https://oidc.initial/user"],
      ["Scopes", "profile", "openid email"],
    ] as const;
    for (const [label, changed] of fields) {
      fireEvent.change(providerInput(oidc, label), {
        target: { value: changed },
      });
    }
    fireEvent.click(within(oidc).getByRole("button", { name: "Reset OIDC" }));
    for (const [label, , initial] of fields) {
      expect(providerInput(oidc, label)).toHaveValue(initial);
    }
    expect(
      within(oidc).getByRole("button", { name: "Remove OIDC" }),
    ).toBeEnabled();
  });

  it("awaits success, preserves pending edits, and resets to the saved baseline", async () => {
    const request = deferred<void>();
    state.setConfig.mockReturnValueOnce(request.promise);
    const { postSubmit } = renderSettings();
    const github = openProvider("GitHub");
    const clientId = providerInput(github, "Client Id");
    fireEvent.change(clientId, { target: { value: "submitted-id" } });
    await save();
    expect(postSubmit).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Saving…" })).toBeDisabled();

    fireEvent.change(clientId, { target: { value: "newer-id" } });
    request.resolve();
    await waitFor(() => expect(postSubmit).toHaveBeenCalledWith(true));
    expect(clientId).toHaveValue("newer-id");
    fireEvent.click(screen.getByRole("button", { name: "Reset" }));
    expect(clientId).toHaveValue("submitted-id");

    fireEvent.change(clientId, { target: { value: "after-success-id" } });
    expect(screen.getByRole("button", { name: "Save changes" })).toBeVisible();
  });

  it("suppresses pending save callbacks after unmount", async () => {
    const request = deferred<void>();
    state.setConfig.mockReturnValueOnce(request.promise);
    const { postSubmit, unmount } = renderSettings();
    const github = openProvider("GitHub");
    fireEvent.change(providerInput(github, "Client Id"), {
      target: { value: "submitted-id" },
    });
    await save();
    unmount();
    request.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(postSubmit).not.toHaveBeenCalled();
  });

  it("rebases clean input and preserves dirty input across query refreshes", async () => {
    renderSettings();
    const github = openProvider("GitHub");
    const clientId = providerInput(github, "Client Id");
    const cleanRemote = initialConfig();
    cleanRemote.auth!.oauthProviders.github!.clientId = "clean-remote-id";
    state.updateConfigQuery!(cleanRemote);
    await waitFor(() => expect(clientId).toHaveValue("clean-remote-id"));
    expect(screen.queryByRole("button", { name: "Save changes" })).toBeNull();

    fireEvent.change(clientId, { target: { value: "local-id" } });
    const dirtyRemote = initialConfig();
    dirtyRemote.auth!.oauthProviders.github!.clientId = "dirty-remote-id";
    dirtyRemote.auth!.refreshTokenTtlSec = 9999n;
    state.updateConfigQuery!(dirtyRemote);
    await waitFor(() => expect(clientId).toHaveValue("local-id"));
    expect(screen.getByRole("button", { name: "Save changes" })).toBeVisible();
  });

  it("three-way merges provider leaves and preserves untouched full Config", async () => {
    renderSettings();
    const github = openProvider("GitHub");
    fireEvent.change(providerInput(github, "Client Id"), {
      target: { value: "local-id" },
    });
    const remote = initialConfig();
    remote.auth!.oauthProviders.github!.displayName = "Remote GitHub";
    remote.auth!.oauthProviders.github!.authUrl = "https://remote/auth";
    remote.auth!.oauthProviders.discord = providerConfig(
      OAuthProviderId.DISCORD,
      { clientId: "remote-discord-id", displayName: "Remote Discord" },
    );
    remote.server = {
      applicationName: "Remote App",
      siteUrl: "https://remote.test",
      enableRecordTransactions: true,
    };
    remote.databases = [{ name: "remote-db" }];
    state.updateConfigQuery!(remote);
    await save();

    const saved = state.setConfig.mock.calls[0][0].config as Config;
    expect(saved.auth!.oauthProviders.github).toMatchObject({
      clientId: "local-id",
      displayName: "Remote GitHub",
      authUrl: "https://remote/auth",
    });
    expect(saved.auth!.oauthProviders.discord).toMatchObject({
      clientId: "remote-discord-id",
      displayName: "Remote Discord",
    });
    expect(saved.server).toMatchObject({
      applicationName: "Remote App",
      siteUrl: "https://remote.test",
      enableRecordTransactions: true,
    });
    expect(saved.databases).toEqual([
      expect.objectContaining({ name: "remote-db" }),
    ]);
  });

  it("reapplies an awaited submission onto refreshed Auth and baseline", async () => {
    const request = deferred<void>();
    state.setConfig.mockReturnValueOnce(request.promise);
    const { postSubmit } = renderSettings();
    const oidc = openProvider("OIDC");
    const clientId = providerInput(oidc, "Client Id");
    fireEvent.change(clientId, { target: { value: "submitted-id" } });
    await save();

    const remote = initialConfig();
    remote.auth!.oauthProviders.oidc0!.authUrl = "https://remote/auth";
    remote.auth!.oauthProviders.oidc0!.tokenUrl = "https://remote/token";
    remote.auth!.oauthProviders.github!.displayName = "Refreshed GitHub";
    remote.auth!.refreshTokenTtlSec = 9999n;
    remote.server = {
      applicationName: "Refreshed App",
      siteUrl: "https://refreshed.test",
      enableRecordTransactions: true,
    };
    remote.databases = [{ name: "refreshed-db" }];
    state.updateConfigQuery!(remote);
    request.resolve();

    await waitFor(() => expect(postSubmit).toHaveBeenCalledWith(false));
    expect(clientId).toHaveValue("submitted-id");
    expect(providerInput(oidc, "Auth URL")).toHaveValue("https://remote/auth");
    expect(providerInput(oidc, "Token URL")).toHaveValue(
      "https://remote/token",
    );

    fireEvent.change(providerInput(oidc, "Auth URL"), {
      target: { value: "https://another/auth" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Reset" }));
    expect(clientId).toHaveValue("submitted-id");
    expect(providerInput(oidc, "Auth URL")).toHaveValue("https://remote/auth");

    fireEvent.change(providerInput(oidc, "Scopes"), {
      target: { value: "openid profile" },
    });
    await save();
    const saved = state.setConfig.mock.calls[1][0].config as Config;
    expect(saved.auth!.oauthProviders.oidc0).toMatchObject({
      clientId: "submitted-id",
      authUrl: "https://remote/auth",
      tokenUrl: "https://remote/token",
      scopes: ["openid", "profile"],
    });
    expect(saved.auth!.oauthProviders.github!.displayName).toBe(
      "Refreshed GitHub",
    );
    expect(saved.auth!.refreshTokenTtlSec).toBe(9999n);
    expect(saved.server!.applicationName).toBe("Refreshed App");
    expect(saved.databases).toEqual([
      expect.objectContaining({ name: "refreshed-db" }),
    ]);
  });

  it("downloads the public key without rendering it", async () => {
    const body = new ReadableStream();
    state.adminFetch.mockResolvedValueOnce({ body });
    renderSettings();
    fireEvent.click(
      screen.getByRole("button", { name: "Download public key" }),
    );
    expect(state.showSaveFileDialog).toHaveBeenCalledOnce();
    const options = state.showSaveFileDialog.mock.calls[0][0];
    expect(options.filename).toBe("public_key.pem");
    await expect(options.contents()).resolves.toBe(body);
    expect(state.adminFetch).toHaveBeenCalledWith("/public_key");
    expect(document.body.textContent).not.toContain("BEGIN PUBLIC KEY");
  });
});
