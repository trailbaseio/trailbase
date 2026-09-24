import { test, describe } from "vitest";
import { status } from "http-status";

import { serverAddress, envVarSet } from "../util";

const includeJsGuest = envVarSet("JS_GUEST_RUNTIME");

type Runtime = "Rust" | "JS";

type Guest = {
  runtime: Runtime;
  base: string;
};

const ONLY_RUST: Guest = {
  runtime: "Rust",
  base: `${serverAddress()}`,
};

const ONLY_JS: Guest = {
  runtime: "JS",
  base: `${serverAddress()}/js`,
};

const GUESTS: Guest[] = includeJsGuest ? [ONLY_RUST, ONLY_JS] : [ONLY_RUST];

test.concurrent.for(GUESTS)(
  "WASM sanity: $runtime",
  async ({ runtime, base }, { expect }) => {
    // Make sure we're calling the right guest;
    expect(
      await (await fetch(`http://${base}/rt`, { method: "GET" })).text(),
    ).toBe(runtime);
  },
);

describe.concurrent.for(GUESTS)("WASM HTTP: $runtime", ({ runtime, base }) => {
  test("sanity", async ({ expect }) => {
    // Make sure we're calling the right guest;
    expect(
      await (await fetch(`http://${base}/rt`, { method: "GET" })).text(),
    ).toBe(runtime);
  });

  // Make sure that methods, routes, paths, params are all properly wired up.
  test("simple incoming HTTP", async ({ expect }) => {
    async function tests() {
      expect(
        await (await fetch(`http://${base}/method`, { method: "GET" })).text(),
      ).toBe("get");
      expect(
        await (await fetch(`http://${base}/method`, { method: "POST" })).text(),
      ).toBe("post");
      expect(
        await (
          await fetch(`http://${base}/method`, { method: "DELETE" })
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

      const jsonUrl = `http://${base}/json`;
      const json = await (await fetch(jsonUrl)).json();
      expect(json).toMatchObject(expected);

      const response = await fetch(
        `http://${base}/fetch?url=${encodeURI(jsonUrl)}`,
      );
      expect(await response.json()).toMatchObject(expected);

      const errResp = await fetch(`http://${base}/error`);
      expect(errResp.status).equals(status.IM_A_TEAPOT);

      // Test that the periodic callback was called.
      expect((await fetch(`http://${base}/await`)).status).equals(status.OK);
    }

    // Run above tests a few times concurrently.
    await Promise.all(
      Array.from({ length: 25 }, async (_v, _i) => await tests()),
    );
  });

  // Make sure the everything works and keeps working (e.g. the shared rt pool
  // doesn't get poisoned) when a component returns a non-http response, e.g.:
  // traps by panicking.
  test("incoming HTTP triggers panic", async ({ expect }) => {
    const panic = async () => {
      const response = await fetch(`http://${base}/panic`);
      expect(response.status).equals(status.INTERNAL_SERVER_ERROR);
    };

    await Promise.all(
      Array.from({ length: 25 }, async (_v, _i) => await panic()),
    );

    // Make sure everything is still working.
    expect(
      await (await fetch(`http://${base}/method`, { method: "GET" })).text(),
    ).toBe("get");
  });

  // Make sure that we have TLS and guests can call external HTTPS targets.
  test("outgoing TLS/HTTPS", async ({ expect }) => {
    const TARGET = "https://example.com";

    const response = await fetch(
      `http://${base}/fetch?url=${encodeURI(TARGET)}`,
    );
    expect(response.ok);
  });
});

describe.concurrent.for(GUESTS)("WASM DB: $runtime", ({ runtime, base }) => {
  async function execute(sql: string) {
    const resp = await fetch(`http://${base}/execute_db/${btoa(sql)}`);
    if (!resp.ok) {
      throw new Error(await resp.text());
    }
  }

  async function execute_batch(sql: string) {
    const resp = await fetch(`http://${base}/execute_batch_db/${btoa(sql)}`);
    if (!resp.ok) {
      throw new Error(await resp.text());
    }
  }

  async function query(sql: string): Promise<string> {
    const resp = await fetch(`http://${base}/query_db/${btoa(sql)}`);
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

  test("simple statements", async ({ expect }) => {
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

  test("attaching/detaching multi DB", async ({ expect }) => {
    // Attach db with invalid name fails
    await expect(async () => await attach("session")).rejects.toThrow();

    const dbName = `db${runtime}`;
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

    // Finally detach to not clobber multiple executions.
    await detach(dbName);
  }, /* timeout= */ 15000);

  test("concurrent query & execute", async ({ expect }) => {
    const responses = await Promise.all(
      Array.from({ length: 25 }, async (_v, _i) => {
        const response = await fetch(`http://${base}/addDeletePost`);

        return await response.text();
      }),
    );

    expect(responses).toHaveLength(25);
    for (const resp of responses) {
      expect(resp).toEqual("Ok");
    }
  });

  test("transactions", async ({ expect }) => {
    await Promise.all(
      Array.from({ length: 25 }, async (_v, _i) => {
        const response = await fetch(`http://${base}/transaction`);
        expect(response.status, `Got: ${await response.text()}`).toBe(200);
      }),
    );
  });

  test("custom SQLite extension functions", async ({ skip, expect }) => {
    // Custom SQLite extension functions are only supported by Rust (yet).
    // However, using JS guests - due to their lack of re-usability - would
    // probably be prohibitively expensive.
    if (runtime == "JS") {
      skip();
    }

    // We call the `sqlite_stateful` endpoint 100 times concurrently, sort the result and check it's (0..99).
    async function getCount(): Promise<number> {
      const response = await fetch(`http://${base}/sqlite_stateful`);

      return parseInt((await response.text()).trim());
    }

    const N = 100;
    const counts: number[] = await Promise.all(
      Array.from({ length: N }, (_v, _i) => getCount()),
    );
    counts.sort((a, b) => a - b);

    expect(counts).toEqual(Array.from({ length: N }, (_v, i) => i));
  });

  test("calling default sqlite-vec extension", async ({ expect }) => {
    await Promise.all(
      Array.from({ length: 25 }, async (_v, _i) => {
        const response = await fetch(`http://${base}/test_sqlite-vec`);
        const b64Vec = (await response.text()).trim();

        expect(b64Vec).toEqual("AAAAAAAAgD8AAABAAABAQA==");
      }),
    );
  });
});
