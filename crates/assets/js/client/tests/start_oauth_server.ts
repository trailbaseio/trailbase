import type { TestProject } from "vitest/node";
import { expect } from "vitest";
import { OAuth2Server } from "oauth2-mock-server";

export type OAuthServerAddress = {
  address: string;
  port: number;
};

declare module "vitest" {
  export interface ProvidedContext {
    oauthServerAddress: OAuthServerAddress;
  }
}

type OpenIdConfig = {
  issuer: string;
  token_endpoint: string;
  authorization_endpoint: string;
  userinfo_endpoint: string;
};

let server: OAuth2Server | null = null;

export async function setup(project: TestProject) {
  server = new OAuth2Server();

  // Generate a new RSA key and add it to the keystore
  await server.issuer.keys.generate("RS256");

  server.service.on("beforeUserinfo", (userInfoResponse, _req) => {
    userInfoResponse.body = {
      sub: "joanadoe",
      email: "joana@doe.org",
      email_verified: true,
    };
    userInfoResponse.statusCode = 200;
  });

  // NOTE: this port needs to match the client/testfixture/config.textproto.
  const [address, port] = ["127.0.0.1", 9088];
  await server.start(port, address);

  // Validate setup.
  const response = await fetch(
    `http://${address}:${port}/.well-known/openid-configuration`,
  );
  const config: OpenIdConfig = await response.json();
  expect(config.token_endpoint).toBe(`http://localhost:${port}/token`);

  project.provide("oauthServerAddress", {
    address,
    port,
  } satisfies OAuthServerAddress);
}

export async function teardown() {
  if (server) {
    await server.stop();
  }
}

declare module "vitest" {
  export interface ProvidedContext {
    oauthServerAddress: OAuthServerAddress;
  }
}
