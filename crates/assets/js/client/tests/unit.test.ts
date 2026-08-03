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

describe("signUp", () => {
  it("registers an email account without signing in", async () => {
    const transport = new RecordingTransport(
      () => new Response("registered", { status: 200 }),
    );
    const client = initClient("http://localhost", { transport });

    await client.signUp({
      email: "alice@example.org",
      password: "s3cr3t!",
    });

    expect(client.user()).toBe(undefined);
    expect(transport.paths).toStrictEqual(["/api/auth/v1/register"]);
    expect(transport.bodies[0]).toStrictEqual({
      email: "alice@example.org",
      username: null,
      password: "s3cr3t!",
      password_repeat: "s3cr3t!",
      redirect_uri: null,
    });
  });

  it("registers a username account and forwards the redirect uri", async () => {
    const transport = new RecordingTransport(
      () => new Response("registered", { status: 200 }),
    );
    const client = initClient("http://localhost", { transport });

    await client.signUp({
      username: "alice",
      password: "s3cr3t!",
      passwordRepeat: "s3cr3t!",
      options: { redirectUri: "my-app://callback" },
    });

    expect(transport.paths).toStrictEqual(["/api/auth/v1/register"]);
    expect(transport.bodies[0]).toStrictEqual({
      email: null,
      username: "alice",
      password: "s3cr3t!",
      password_repeat: "s3cr3t!",
      redirect_uri: "my-app://callback",
    });
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
