import { describe, expect, it } from "vitest";
import {
  AuthConfig,
  OAuthProviderConfig,
  OAuthProviderId,
} from "@proto/config";
import {
  configToProxy,
  proxyToConfig,
} from "@/components/settings/AuthSettings";

const provider = {
  id: OAuthProviderId.GITHUB,
  name: "github",
  display_name: "GitHub",
};

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
});
