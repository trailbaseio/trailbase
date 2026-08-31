import { describe, expect, it } from "vitest";
import { DatabaseConfig } from "@proto/config";
import { validateDatabaseName } from "@/components/settings/DatabaseSettings";

const existing = [DatabaseConfig.fromPartial({ name: "analytics" })];

describe("database settings validation", () => {
  it.each([
    ["", "Enter a database name."],
    ["   ", "Enter a database name."],
    ["main", "That database name is reserved."],
    ["public", "That database name is reserved."],
    ["logs", "That database name is reserved."],
    ["session", "That database name is reserved."],
    ["analytics", "That database is already linked."],
    ["has space", "Use only letters, numbers, underscores, and hyphens."],
    ["a/b", "Use only letters, numbers, underscores, and hyphens."],
    ["ümlaut", "Use only letters, numbers, underscores, and hyphens."],
  ])("rejects %j", (name, error) => {
    expect(validateDatabaseName(name, existing)).toBe(error);
  });

  it.each(["metrics", "metrics_2", "metrics-prod", "A1"])(
    "accepts %s",
    (name) => expect(validateDatabaseName(name, existing)).toBeUndefined(),
  );

  it("trims accepted names and validates duplicate trimmed names", () => {
    expect(validateDatabaseName(" metrics ", existing)).toBeUndefined();
    expect(validateDatabaseName(" analytics ", existing)).toBe(
      "That database is already linked.",
    );
  });
});
