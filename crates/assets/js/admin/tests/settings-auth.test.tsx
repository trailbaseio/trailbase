import { describe, expect, it } from "vitest";
import {
  AuthConfig,
  OAuthProviderConfig,
  OAuthProviderId,
} from "@proto/config";
import {
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
    expect(merged.oauthProviders.github).toEqual(remoteProvider);
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
