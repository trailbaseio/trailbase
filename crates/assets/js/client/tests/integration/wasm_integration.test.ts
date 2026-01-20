import { expect, test } from "vitest";
import { status } from "http-status";
import { ADDRESS } from "../constants";

test("WASM runtime", async () => {
  const expected = {
    int: 5,
    real: 4.2,
    msg: "foo",
    obj: {
      nested: true,
    },
  };

  const jsonUrl = `http://${ADDRESS}/json`;
  const json = await (await fetch(jsonUrl)).json();
  expect(json).toMatchObject(expected);

  const response = await fetch(
    `http://${ADDRESS}/fetch?url=${encodeURI(jsonUrl)}`,
  );
  expect(await response.json()).toMatchObject(expected);

  const errResp = await fetch(`http://${ADDRESS}/error`);
  expect(errResp.status).equals(status.IM_A_TEAPOT);

  // Test that the periodic callback was called.
  expect((await fetch(`http://${ADDRESS}/await`)).status).equals(status.OK);
});

test("WASM runtime DB Query & Execute", async () => {
  const response = await (
    await fetch(`http://${ADDRESS}/addDeletePost`)
  ).text();
  expect(response).toEqual("Ok");
});

test("WASM runtime custom SQLite extension functions", async () => {
  // We call the stateful count endpoint 100 times concurrently, sort the result and check it's (0..99).

  async function getCount(): Promise<number> {
    const response = await fetch(`http://${ADDRESS}/sqlite_stateful`);

    return parseInt((await response.text()).trim());
  }

  const N = 100;
  const counts: number[] = await Promise.all(
    Array.from({ length: N }, (_v, _i) => getCount()),
  );
  counts.sort((a, b) => a - b);

  expect(counts).toEqual(Array.from({ length: N }, (_v, i) => i));
});
