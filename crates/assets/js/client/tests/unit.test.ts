import { test } from "vitest";
import { describe, it, expect } from "vitest";

import { FetchError, initClient } from "../src/index";
import type { Transport } from "../src/index";
import { parseJSON } from "../src/json";
import {
  exportedForTesting,
  ChangeEventStatusForbidden,
  isNull,
  isNotNull,
} from "../src/record_api";

const { parseChangeEvent } = exportedForTesting!;

test("error-handling", async ({ expect }) => {
  expect(new FetchError(404, "test", "url").toString()).toEqual(
    "FetchError(404, test, url)",
  );

  const client = initClient("http://localhost:34444");

  // This is the actual `fetch()` failing to connect, i.e. throwing rather than yielding an error response.
  await expect(
    async () => await client.login("foo", "bar"),
  ).rejects.toThrowError(new TypeError("fetch failed"));
});

test("BigInt JSON parsing", ({ expect }) => {
  const huge = BigInt("0x1fffffffffffff"); // 9007199254740991n

  // Make sure we're actually beyond number precision.
  const clipped: number = Number(huge);
  expect(huge).not.toBe(clipped);

  const json = `{ "value": ${huge} }`;
  const obj: { value: bigint } = parseJSON(json);
  expect(obj.value, json).not.toBe(huge);
});

test("ChangeEvent parsing", ({ expect }) => {
  const json = `{
          "Error": {
            "status": 1,
            "message": "test"
          },
          "seq": 3
         }`;

  expect(parseChangeEvent(`data: ${json}`)).toStrictEqual({
    seq: 3,
    Error: {
      status: ChangeEventStatusForbidden,
      message: "test",
    },
  });
});

class RecordingTransport implements Transport {
  public readonly paths: string[] = [];
  public readonly bodies: Record<string, unknown>[] = [];

  constructor(
    private readonly handler: (path: string) => Response,
    private readonly base = "http://localhost",
  ) {}

  async fetch(path: string, init?: RequestInit): Promise<Response> {
    this.paths.push(new URL(path, this.base).pathname);
    this.bodies.push(JSON.parse((init?.body as string | undefined) ?? "null"));
    return this.handler(path);
  }
}

function urlSafeBase64(value: object): string {
  return btoa(JSON.stringify(value))
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/, "");
}

/// Builds an unsigned, decodable JWT. `jwt-decode` does not verify signatures.
function fakeAuthToken(email: string | null, username: string | null): string {
  const now = Math.floor(Date.now() / 1000);
  const claims = {
    sub: "6a8f0e6c-1a3b-7cde-8000-000000000000",
    iat: now,
    exp: now + 3600,
    email,
    username,
    csrf_token: "csrf",
  };
  return `${urlSafeBase64({ alg: "none", typ: "JWT" })}.${urlSafeBase64(claims)}.`;
}

function loginResponse(
  email: string | null,
  username: string | null,
): Response {
  return new Response(
    JSON.stringify({
      auth_token: fakeAuthToken(email, username),
      refresh_token: "refresh",
      csrf_token: "csrf",
    }),
    { status: 200 },
  );
}

describe("signUp", () => {
  it("posts the registration and requires verification for email accounts", async () => {
    const transport = new RecordingTransport(
      () => new Response("registered", { status: 200 }),
    );
    const client = initClient("http://localhost", { transport });

    const response = await client.signUp({
      email: "alice@example.org",
      password: "s3cr3t!",
    });

    expect(response).toStrictEqual({ verificationRequired: true });
    expect(client.user()).toBe(undefined);

    // No implicit sign-in, since the email needs to be verified first.
    expect(transport.paths).toStrictEqual(["/api/auth/v1/register"]);
    expect(transport.bodies[0]).toStrictEqual({
      email: "alice@example.org",
      username: null,
      password: "s3cr3t!",
      password_repeat: "s3cr3t!",
      redirect_uri: null,
    });
  });

  it("signs in username-only accounts", async () => {
    const transport = new RecordingTransport((path) =>
      path.endsWith("/login")
        ? loginResponse(null, "alice")
        : new Response("registered", { status: 200 }),
    );
    const client = initClient("http://localhost", { transport });

    const response = await client.signUp({
      username: "alice",
      password: "s3cr3t!",
      passwordRepeat: "s3cr3t!",
      options: { redirectUri: "my-app://callback" },
    });

    expect(response.verificationRequired).toBe(false);
    expect(response.user?.username).toBe("alice");
    expect(client.user()?.username).toBe("alice");

    expect(transport.paths).toStrictEqual([
      "/api/auth/v1/register",
      "/api/auth/v1/login",
    ]);
    expect(transport.bodies[0]).toStrictEqual({
      email: null,
      username: "alice",
      password: "s3cr3t!",
      password_repeat: "s3cr3t!",
      redirect_uri: "my-app://callback",
    });
  });

  it("skips the implicit sign-in when autoLogin is off", async () => {
    const transport = new RecordingTransport(
      () => new Response("registered", { status: 200 }),
    );
    const client = initClient("http://localhost", { transport });

    const response = await client.signUp({
      username: "alice",
      password: "s3cr3t!",
      options: { autoLogin: false },
    });

    expect(response).toStrictEqual({
      user: undefined,
      verificationRequired: false,
    });
    expect(transport.paths).toStrictEqual(["/api/auth/v1/register"]);
  });

  it("tolerates a rejected sign-in for an already existing account", async () => {
    const transport = new RecordingTransport((path) =>
      path.endsWith("/login")
        ? new Response("unauthorized", { status: 401 })
        : new Response("registered", { status: 200 }),
    );
    const client = initClient("http://localhost", { transport });

    // Registration reports success even when the account already exists, so the
    // subsequent sign-in may legitimately fail. That must not throw.
    const response = await client.signUp({
      username: "alice",
      password: "wrong-password",
    });

    expect(response).toStrictEqual({
      user: undefined,
      verificationRequired: false,
    });
    expect(transport.paths).toStrictEqual([
      "/api/auth/v1/register",
      "/api/auth/v1/login",
    ]);
  });

  it("propagates registration errors", async () => {
    const transport = new RecordingTransport(
      () => new Response("Missing email", { status: 400 }),
    );
    const client = initClient("http://localhost", { transport });

    const err = await client.signUp({ password: "s3cr3t!" }).then(
      () => undefined,
      (err: unknown) => err,
    );

    expect(err).toBeInstanceOf(FetchError);
    expect((err as FetchError).status).toBe(400);
    expect((err as FetchError).msg).toBe("Missing email");
  });
});

describe("filter $is", () => {
  it("serializes isNull to $is=NULL", () => {
    const p = new URLSearchParams();
    exportedForTesting!.addFiltersToParams(p, "filter", isNull("col0"));
    expect(p.get("filter[col0][$is]")).toBe("NULL");
  });

  it("serializes isNotNull to $is=!NULL", () => {
    const p = new URLSearchParams();
    exportedForTesting!.addFiltersToParams(p, "filter", isNotNull("col0"));
    expect(p.get("filter[col0][$is]")).toBe("!NULL");
  });

  it("works inside an And composite", () => {
    const p = new URLSearchParams();
    exportedForTesting!.addFiltersToParams(p, "filter", {
      and: [isNull("a"), isNotNull("b")],
    });
    expect(p.get("filter[$and][0][a][$is]")).toBe("NULL");
    expect(p.get("filter[$and][1][b][$is]")).toBe("!NULL");
  });
});
