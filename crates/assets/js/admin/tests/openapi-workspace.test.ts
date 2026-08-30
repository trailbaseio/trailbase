import { describe, expect, it } from "vitest";

import {
  applyRequestTokens,
  openApiMetadata,
  parseImpersonationTokens,
  usableRequestTokens,
  withCollapsedOpenApiTags,
} from "../src/components/openapi/openapi";

describe("OpenAPI metadata", () => {
  it("counts only HTTP operations and falls back safely", () => {
    expect(
      openApiMetadata({
        info: { title: "API", version: "1" },
        paths: {
          "/users": { get: {}, POST: {}, parameters: {}, summary: "ignored" },
          "/users/{id}": { delete: {}, "x-extra": {}, $ref: "ignored" },
        },
      }),
    ).toEqual({ title: "API", version: "1", operationCount: 3 });
    expect(openApiMetadata({ info: null, paths: null })).toEqual({
      title: "OpenAPI",
      operationCount: 0,
    });
    expect(openApiMetadata({ info: { title: 4 }, paths: [] })).toEqual({
      title: "OpenAPI",
      operationCount: 0,
    });
  });
});

describe("collapsed OpenAPI tags", () => {
  it("clones only the tag containers and preserves the source", () => {
    const source = {
      openapi: "3.0.0",
      tags: [{ name: "users", description: "Users" }, { name: "admin" }],
      paths: { "/users": { get: { tags: ["users"] } } },
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
    expect(result.paths).toBe(source.paths);
    expect(source.tags).toEqual([
      { name: "users", description: "Users" },
      { name: "admin" },
    ]);
    expect(withCollapsedOpenApiTags({ tags: null })).toEqual({ tags: null });
  });
});

describe("request token trust boundary", () => {
  const tokens = {
    auth_token: "auth-secret-value",
    refresh_token: "refresh-secret-value",
    csrf_token: "csrf-secret-value",
  };

  it("distinguishes empty, valid, and invalid impersonation input without leaking tokens", () => {
    expect(parseImpersonationTokens("")).toBeNull();
    expect(parseImpersonationTokens(btoa(JSON.stringify(tokens)))).toEqual(
      tokens,
    );
    for (const value of [
      "not-base64",
      btoa("[]"),
      btoa(JSON.stringify({ ...tokens, csrf_token: null })),
    ]) {
      expect(() => parseImpersonationTokens(value)).toThrow(
        "Invalid login tokens",
      );
      try {
        parseImpersonationTokens(value);
      } catch (error) {
        expect(String(error)).not.toContain(tokens.auth_token);
        expect(String(error)).not.toContain(tokens.refresh_token);
        expect(String(error)).not.toContain(tokens.csrf_token);
      }
    }
  });

  it("rejects nullable or incomplete current tokens", () => {
    expect(usableRequestTokens(tokens)).toEqual(tokens);
    expect(usableRequestTokens({ ...tokens, refresh_token: null })).toBeNull();
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
