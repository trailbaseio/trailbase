import { describe, expect, it } from "vitest";
import {
  openApiMetadata,
  parseImpersonationTokens,
} from "../src/components/openapi/openapi";

describe("OpenAPI workspace behavior", () => {
  it("reports operation metadata and token validation", () => {
    expect(
      openApiMetadata({
        info: { version: "1" },
        paths: { "/x": { get: {}, post: {} } },
      }),
    ).toMatchObject({ version: "1", operationCount: 2 });
    expect(parseImpersonationTokens(" ")).toBeNull();
    expect(() => parseImpersonationTokens("not-valid")).toThrow(
      "Invalid login tokens",
    );
  });
});
