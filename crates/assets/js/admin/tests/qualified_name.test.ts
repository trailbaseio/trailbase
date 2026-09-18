import { test } from "vitest";

import { parseQualifiedName } from "@/lib/qualified_name";
import type { QualifiedName } from "@bindings/QualifiedName";

test("QualifiedName parsing", ({ expect }) => {
  expect(parseQualifiedName("test")).toEqual({
    name: "test",
    database_schema: null,
  } satisfies QualifiedName);
  expect(parseQualifiedName("`test`")).toEqual({
    name: "test",
    database_schema: null,
  } satisfies QualifiedName);

  expect(parseQualifiedName("db.test")).toEqual({
    name: "test",
    database_schema: "db",
  } satisfies QualifiedName);

  expect(parseQualifiedName("`db`.[test]")).toEqual({
    name: "test",
    database_schema: "db",
  } satisfies QualifiedName);

  expect(parseQualifiedName("db.[test]")).toEqual({
    name: "test",
    database_schema: "db",
  } satisfies QualifiedName);

  expect(parseQualifiedName("'db'.test")).toEqual({
    name: "test",
    database_schema: "db",
  } satisfies QualifiedName);

  expect(parseQualifiedName("'db'.test")).toEqual({
    name: "test",
    database_schema: "db",
  } satisfies QualifiedName);

  expect(() => parseQualifiedName("'db'.test.")).throws();
  expect(() => parseQualifiedName("test;inject")).throws();
});
