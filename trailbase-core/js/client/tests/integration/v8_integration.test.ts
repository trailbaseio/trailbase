import { expect, test } from "vitest";
import { status } from "http-status";

const port: number = 4005;
const address: string = `http://127.0.0.1:${port}`;

test("JS runtime", async () => {
  const expected = {
    int: 5,
    real: 4.2,
    msg: "foo",
    obj: {
      nested: true,
    },
  };

  const jsonUrl = `${address}/json`;
  const json = await (await fetch(jsonUrl)).json();
  expect(json).toMatchObject(expected);

  const response = await fetch(`${address}/fetch?url=${encodeURI(jsonUrl)}`);
  expect(await response.json()).toMatchObject(expected);

  const errResp = await fetch(`${address}/error`);
  expect(errResp.status).equals(status.IM_A_TEAPOT);

  // Test that the periodic callback was called.
  expect((await fetch(`${address}/await`)).status).equals(status.OK);
});
