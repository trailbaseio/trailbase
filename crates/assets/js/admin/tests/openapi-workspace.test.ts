import { describe, expect, it } from "vitest";

import {
  applyRequestTokens,
  openApiMetadata,
  parseImpersonationTokens,
  usableRequestTokens,
  withCollapsedOpenApiTags,
} from "../src/components/openapi/openapi";

const tokens = {
  auth_token: "auth-secret-value",
  refresh_token: "refresh-secret-value",
  csrf_token: "csrf-secret-value",
};
const encode = (value: unknown) => btoa(JSON.stringify(value));

describe("OpenAPI metadata", () => {
  it("counts supported methods case-insensitively and ignores path metadata", () => {
    const methods = [
      "get",
      "PUT",
      "Post",
      "DELETE",
      "patch",
      "OPTIONS",
      "HEAD",
      "TRACE",
    ];
    expect(
      openApiMetadata({
        info: { title: "API", version: "1" },
        paths: {
          "/all": Object.fromEntries(methods.map((method) => [method, {}])),
          "/ignored": {
            parameters: {},
            summary: "",
            description: "",
            servers: [],
            $ref: "",
            "x-method": {},
          },
        },
      }),
    ).toEqual({ title: "API", version: "1", operationCount: 8 });
  });

  it("falls back safely for malformed documents", () => {
    for (const spec of [
      undefined,
      null,
      [],
      "invalid",
      { info: null },
      { paths: [] },
    ]) {
      expect(openApiMetadata(spec)).toEqual({
        title: "OpenAPI",
        operationCount: 0,
      });
    }
  });
});

describe("collapsed OpenAPI tags", () => {
  it("clones tags without rewriting other source references", () => {
    const paths = { "/users": { get: { tags: ["users"] } } };
    const source = {
      openapi: "3.0.0",
      tags: [{ name: "users", description: "Users" }, { name: "admin" }],
      paths,
    };
    const result = withCollapsedOpenApiTags(source);
    expect(result).toEqual({
      ...source,
      tags: [
        { name: "users", description: "Users", "x-tag-expanded": false },
        { name: "admin", "x-tag-expanded": false },
      ],
    });
    expect(result).not.toBe(source);
    expect(result.tags).not.toBe(source.tags);
    expect((result.tags as unknown[])[0]).not.toBe(source.tags?.[0]);
    expect(result.paths).toBe(paths);
    expect(source.tags).toEqual([
      { name: "users", description: "Users" },
      { name: "admin" },
    ]);
  });

  it("handles malformed or absent tag containers safely", () => {
    for (const spec of [
      undefined,
      null,
      [],
      "invalid",
      {},
      { tags: null },
      { tags: {} },
      { tags: [null, "bad"] },
    ]) {
      const result = withCollapsedOpenApiTags(spec);
      expect(result).toBeDefined();
    }
    expect(
      withCollapsedOpenApiTags({ tags: { bad: true }, paths: { ref: {} } })
        .paths,
    ).toEqual({ ref: {} });
  });
});

describe("request token trust boundary", () => {
  it("distinguishes empty, valid, and invalid input without leaking tokens", () => {
    expect(parseImpersonationTokens("")).toBeNull();
    expect(parseImpersonationTokens("  \t\n ")).toBeNull();
    expect(parseImpersonationTokens(`  ${encode(tokens)}  `)).toEqual(tokens);
    const invalid = [
      "not-base64",
      "eyJmb28iOiJiYXIifQ==",
      "W10=",
      encode({}),
      ...["auth_token", "refresh_token", "csrf_token"].map((key) =>
        encode(
          Object.fromEntries(
            Object.entries(tokens).filter(([name]) => name !== key),
          ),
        ),
      ),
      encode({ ...tokens, auth_token: 1 }),
      encode({ ...tokens, refresh_token: false }),
      encode({ ...tokens, csrf_token: null }),
      encode({ ...tokens, extra: "nope" }),
      encode(null),
      encode([]),
      btoa("not json"),
    ];
    for (const value of invalid) {
      expect(() => parseImpersonationTokens(value)).toThrow(
        "Invalid login tokens",
      );
      try {
        parseImpersonationTokens(value);
      } catch (error) {
        for (const token of Object.values(tokens))
          expect(String(error)).not.toContain(token);
      }
    }
  });

  it("rejects nullable or incomplete current tokens", () => {
    expect(usableRequestTokens(tokens)).toEqual(tokens);
    expect(usableRequestTokens({ ...tokens, refresh_token: null })).toBeNull();
    expect(
      usableRequestTokens({ ...tokens, csrf_token: 1 } as never),
    ).toBeNull();
    expect(usableRequestTokens(null)).toBeNull();
    expect(usableRequestTokens(undefined)).toBeNull();
  });

  it("replaces exactly the request authentication headers", () => {
    const request = new Request("https://example.test", {
      headers: {
        Authorization: "old",
        "Refresh-Token": "old",
        "CSRF-Token": "old",
        Other: "keep",
      },
    });
    applyRequestTokens(request, tokens);
    expect([...request.headers.entries()]).toEqual([
      ["authorization", "Bearer auth-secret-value"],
      ["csrf-token", "csrf-secret-value"],
      ["other", "keep"],
      ["refresh-token", "refresh-secret-value"],
    ]);
  });
});
