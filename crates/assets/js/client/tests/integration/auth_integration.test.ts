import { expect, test } from "vitest";
import { OAuth2Server } from "oauth2-mock-server";
import { ADDRESS, PORT } from "../constants";

type OpenIdConfig = {
  issuer: string;
  token_endpoint: string;
  authorization_endpoint: string;
  userinfo_endpoint: string;
};

// NOTE: Having this server test live alongside the client is a bit odd.
test("OIDC", async () => {
  if (PORT === 4000) {
    return;
  }

  const server = new OAuth2Server();

  // Generate a new RSA key and add it to the keystore
  await server.issuer.keys.generate("RS256");

  // NOTE: this port needs to match the client/testfixture/config.textproto.
  const authPort = 9088;
  const authAddress = "127.0.0.1";
  await server.start(authPort, authAddress);

  const response = await fetch(
    `http://${authAddress}:${authPort}/.well-known/openid-configuration`,
  );
  const config: OpenIdConfig = await response.json();
  expect(config.token_endpoint).toBe(`http://localhost:${authPort}/token`);

  server.service.on("beforeUserinfo", (userInfoResponse, _req) => {
    userInfoResponse.body = {
      sub: "joanadoe",
      email: "joana@doe.org",
      email_verified: true,
    };
    userInfoResponse.statusCode = 200;
  });

  const login = await fetch(`http://${ADDRESS}/api/auth/v1/oauth/oidc0/login`, {
    redirect: "manual",
  });

  expect(login.status).toBe(303);
  const location = login.headers.get("location")!;
  expect(location).toContain(`http://localhost:${authPort}/authorize`);
  const stateCookie = login.headers.get("set-cookie")!.split(";")[0];

  const authorize = await fetch(location, { redirect: "manual" });

  expect(authorize.status).toBe(302);
  const callbackUrl = authorize.headers.get("location")!;
  const callback = await fetch(callbackUrl, {
    redirect: "manual",
    credentials: "include",
    headers: {
      cookie: stateCookie,
    },
  });

  expect(callback.status).toBe(303);
  expect(callback.headers.get("location")).toBe("/_/auth/profile");

  const authHeader = callback.headers.get("set-cookie")!;
  expect(authHeader)
    .to.be.a("string")
    .and.match(new RegExp(".*auth_token=ey.*"));

  await server.stop();
});
