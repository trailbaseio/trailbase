import { expect, test, describe } from "vitest";
import { status } from "http-status";

import { serverAddress } from "../setup";

test("WASM runtime", async () => {
  async function tests() {
    expect(
      await (
        await fetch(`http://${serverAddress()}/method`, { method: "GET" })
      ).text(),
    ).toBe("get");
    expect(
      await (
        await fetch(`http://${serverAddress()}/method`, { method: "POST" })
      ).text(),
    ).toBe("post");
    expect(
      await (
        await fetch(`http://${serverAddress()}/method`, { method: "DELETE" })
      ).text(),
    ).toBe("delete");

    const expected = {
      int: 5,
      real: 4.2,
      msg: "foo",
      obj: {
        nested: true,
      },
    };

    const jsonUrl = `http://${serverAddress()}/json`;
    const json = await (await fetch(jsonUrl)).json();
    expect(json).toMatchObject(expected);

    const response = await fetch(
      `http://${serverAddress()}/fetch?url=${encodeURI(jsonUrl)}`,
    );
    expect(await response.json()).toMatchObject(expected);

    const errResp = await fetch(`http://${serverAddress()}/error`);
    expect(errResp.status).equals(status.IM_A_TEAPOT);

    // Test that the periodic callback was called.
    expect((await fetch(`http://${serverAddress()}/await`)).status).equals(
      status.OK,
    );
  }

  // Run above tests a few times concurrently.
  await Promise.all(
    Array.from({ length: 25 }, async (_v, _i) => await tests()),
  );
});

test("WASM runtime outgoing TLS/HTTPS", async ({ expect }) => {
  const response = await fetch(
    `http://${serverAddress()}/fetch?url=${encodeURI("https://example.com")}`,
  );
  expect(response.ok);
});

test("WASM runtime DB Query & Execute", async ({ expect }) => {
  const responses = await Promise.all(
    Array.from({ length: 25 }, async (_v, _i) => {
      // const response = await fetch(`http://${serverAddress()}/js/addDeletePost`);
      const response = await fetch(`http://${serverAddress()}/addDeletePost`);

      return await response.text();
    }),
  );

  expect(responses).toHaveLength(25);
  for (const resp of responses) {
    expect(resp).toEqual("Ok");
  }
});

test("WASM runtime DB Transaction", async ({ expect }) => {
  await Promise.all(
    Array.from({ length: 25 }, async (_v, _i) => {
      const response = await fetch(`http://${serverAddress()}/transaction`);
      expect(response.status, `Got: ${await response.text()}`).toBe(200);
    }),
  );
});

test("WASM runtime custom SQLite extension functions", async () => {
  // We call the stateful count endpoint 100 times concurrently, sort the result and check it's (0..99).
  async function getCount(): Promise<number> {
    const response = await fetch(`http://${serverAddress()}/sqlite_stateful`);

    return parseInt((await response.text()).trim());
  }

  const N = 100;
  const counts: number[] = await Promise.all(
    Array.from({ length: N }, (_v, _i) => getCount()),
  );
  counts.sort((a, b) => a - b);

  expect(counts).toEqual(Array.from({ length: N }, (_v, i) => i));
});

test("WASM runtime calling sqlite-vec", async () => {
  await Promise.all(
    Array.from({ length: 25 }, async (_v, _i) => {
      const response = await fetch(`http://${serverAddress()}/test_sqlite-vec`);
      const b64Vec = (await response.text()).trim();

      expect(b64Vec).toEqual("AAAAAAAAgD8AAABAAABAQA==");
    }),
  );
});

describe("WASM runtime sqlite", () => {
  async function execute(sql: string) {
    const resp = await fetch(
      `http://${serverAddress()}/execute_db/${btoa(sql)}`,
    );
    if (!resp.ok) {
      throw new Error(await resp.text());
    }
  }

  async function execute_batch(sql: string) {
    const resp = await fetch(
      `http://${serverAddress()}/execute_batch_db/${btoa(sql)}`,
    );
    if (!resp.ok) {
      throw new Error(await resp.text());
    }
  }

  async function query(sql: string): Promise<string> {
    const resp = await fetch(`http://${serverAddress()}/query_db/${btoa(sql)}`);
    if (resp.ok) {
      return await resp.text();
    }
    throw new Error(await resp.text());
  }

  async function attach(name: string) {
    await execute(`ATTACH DATABASE '${name}.db' AS '${name}';`);
  }

  async function detach(name: string) {
    await execute(`DETACH DATABASE '${name}';`);
  }

  test("simple statements", async () => {
    const countSql = "SELECT COUNT(*) FROM '_user'";
    await query(countSql);
    // Multiple statements throws.
    await expect(
      async () => await query(`${countSql};${countSql};`),
    ).rejects.toThrow();

    const createTableSql =
      "CREATE TABLE IF NOT EXISTS 'test-table' (id INTEGER PRIMARY KEY) STRICT";
    await execute(createTableSql);
    // Multiple statements throws.
    await expect(
      async () => await execute(`${createTableSql};${createTableSql};`),
    ).rejects.toThrow();

    // Test batch
    await execute_batch(countSql);
    await execute_batch(`${createTableSql};${createTableSql};${countSql}`);
  });

  test("attaching/detaching multi DB", async () => {
    // Attach db with invalid name fails
    await expect(async () => await attach("session")).rejects.toThrow();

    const dbName = "foo";
    const tableName = `'${dbName}'.'test'`;

    await attach(dbName);
    await execute(`DROP TABLE IF EXISTS ${tableName};`);
    await execute(`CREATE TABLE ${tableName} (id INTEGER PRIMARY KEY)`);

    for (let i = 0; i < 100; ++i) {
      await execute(
        `INSERT INTO ${tableName} (id) VALUES (${i * 3 + 0}), (${i * 3 + 1}), (${i * 3 + 2});`,
      );
    }

    await detach(dbName);
    // Repeat detach fails:
    await expect(async () => await detach(dbName)).rejects.toThrow();

    // COUNT fails after detach
    await expect(
      async () => await query(`SELECT COUNT(*) FROM ${tableName};`),
    ).rejects.toThrow();

    // And succeeds after re-attach.
    await attach(dbName);
    expect(await query(`SELECT COUNT(*) FROM ${tableName};`)).toEqual("300");
  });
});
