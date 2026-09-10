import { expect, test } from "vitest";
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

test("WASM runtime attaching/detaching multi DB", async () => {
  {
    // Invalid db name:
    const resp0 = await fetch(`http://${serverAddress()}/attach_db/session`);
    const body0 = await resp0.text();
    expect(resp0.status, `Got: ${body0}`).toBe(500);
    expect(body0).toContain("invalid db name");
  }

  const dbName = "foo";
  const tableName = `'${dbName}'.'test'`;

  {
    const resp = await fetch(`http://${serverAddress()}/attach_db/${dbName}`);
    expect(resp.status, `Got: ${await resp.text()}`).toBe(200);
  }

  {
    const sql = `DROP TABLE IF EXISTS ${tableName};`;
    const resp = await fetch(
      `http://${serverAddress()}/execute_db/${btoa(sql)}`,
    );
    expect(resp.status, `Got: ${await resp.text()}`).toBe(200);
  }

  {
    const sql = `CREATE TABLE ${tableName} (id INTEGER PRIMARY KEY)`;
    const resp = await fetch(
      `http://${serverAddress()}/execute_db/${btoa(sql)}`,
    );
    expect(resp.status, `Got: ${await resp.text()}`).toBe(200);
  }

  for (let i = 0; i < 100; ++i) {
    const sql = `INSERT INTO ${tableName} (id) VALUES (${i * 3 + 0}), (${i * 3 + 1}), (${i * 3 + 2});`;
    const resp = await fetch(
      `http://${serverAddress()}/execute_db/${btoa(sql)}`,
    );
    expect(resp.status, `Got: ${await resp.text()}`).toBe(200);
  }

  {
    const resp0 = await fetch(`http://${serverAddress()}/detach_db/${dbName}`);
    expect(resp0.status, `Got: ${await resp0.text()}`).toBe(200);

    const resp1 = await fetch(`http://${serverAddress()}/detach_db/${dbName}`);
    expect(resp1.status, `Got: ${await resp1.text()}`).toBe(500);
  }

  {
    // COUNT fails after detach
    const sql = `SELECT COUNT(*) FROM ${tableName};`;
    const resp = await fetch(`http://${serverAddress()}/query_db/${btoa(sql)}`);
    const body = await resp.text();
    expect(resp.status, `Got: ${body}`).toBe(500);
  }

  {
    const resp = await fetch(`http://${serverAddress()}/attach_db/${dbName}`);
    expect(resp.status, `Got: ${await resp.text()}`).toBe(200);
  }

  {
    const sql = `SELECT COUNT(*) FROM ${tableName};`;
    const resp = await fetch(`http://${serverAddress()}/query_db/${btoa(sql)}`);
    const body = await resp.text();
    expect(resp.status, `Got: ${body}`).toBe(200);
    expect(body, `Got: ${body}`).toEqual("300");
  }
});
