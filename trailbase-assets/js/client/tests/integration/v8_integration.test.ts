import { expect, test } from "vitest";
import { status } from "http-status";
import { ADDRESS } from "../constants";

test("JS runtime", async () => {
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
