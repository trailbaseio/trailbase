import { expect, test, inject } from "vitest";

import { serverAddress, serverPort } from "../util";

// Skip oauth/OIDC test for tests against external TB instances, because the
// server's OIDC provider config, needs to match the test setup.
test.skipIf(serverPort() === 4000)("OIDC", async () => {
  const address = inject("oauthServerAddress");

  const redirectUri = "/_/auth/expected";
  const login = await fetch(
    `http://${serverAddress()}/api/auth/v1/oauth/oidc0/login?redirect_uri=${redirectUri}`,
    {
      redirect: "manual",
    },
  );

  expect(login.status).toBe(303);
  const location = login.headers.get("location")!;
  expect(location).toContain(`http://localhost:${address.port}/authorize`);
  const stateCookie = login.headers.get("set-cookie")!.split(";")[0];

  // NOTE: The fake OAuth provider uses a 302, we use 303 which has more consistent semantics across browsers.
  const authorize = await fetch(location, { redirect: "manual" });
  expect(authorize.status).toBe(302);

  // The redirect by the Auth-UI is constructed using the `config.server.site_url`, which is set to `localhost.trailbase.io:4000`.
  // Unless we want to change the config for each test setup, we're rewriting the address here.
  const callbackUrl = authorize.headers.get("location")!;
  const expected = "http://localhost.trailbase.io:4000";
  expect(callbackUrl).contains(expected);

  const callback = await fetch(
    callbackUrl.replace(expected, `http://${serverAddress()}`),
    {
      redirect: "manual",
      credentials: "include",
      headers: {
        cookie: stateCookie,
      },
    },
  );

  expect(callback.status).toBe(303);
  expect(callback.headers.get("location")).toBe(redirectUri);

  const authHeader = callback.headers.get("set-cookie")!;
  expect(authHeader)
    .to.be.a("string")
    .and.match(new RegExp(".*auth_token=ey.*"));
});
