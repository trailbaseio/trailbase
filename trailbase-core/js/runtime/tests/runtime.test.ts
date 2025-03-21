import { test, expect } from "vitest";

import { addPeriodicCallback } from "../src/trailbase";

globalThis.rustyscript = {
  async_functions: {
  },
  functions: {
    isolate_id: () => 0,
  },
};

test("periodic callback", async () => {
  const promise = new Promise((resolve) => {
    let count = 0;
    const result : number[] = [];

    addPeriodicCallback(1, (cancel) => {
      result.push(count++);

      if (result.length > 2) {
        resolve(result);
        cancel();
      }
    });
  });

  expect(await promise).toEqual([0, 1, 2]);
});
