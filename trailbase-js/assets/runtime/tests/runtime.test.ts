import { test, expect } from "vitest";

import { addPeriodicCallback, parsePath, query, execute, stringHandler, htmlHandler, jsonHandler, addRoute, dispatch, HttpError } from "../src/trailbase";
import type { Method, RequestType, StringRequestType, CallbackType } from "../src/trailbase";
import { decodeFallback, encodeFallback } from "../src/util";

globalThis.rustyscript = {
  async_functions: {},
  functions: {
    isolate_id: () => 0,
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    install_route: (_method: Method, _route: string) => { },
  },
};

test("periodic callback", async () => {
  const promise = new Promise((resolve) => {
    let count = 0;
    const result: number[] = [];

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

test("binary encode/decode", () => {
  const a = `1234567890-=qwertyuiop[]asdfghjkl;'zxcvbnm,./~!@#$%^&*()_+ `;
  expect(decodeFallback(encodeFallback(a))).toEqual(a);
});

test("parse path", () => {
  const parsedPath = parsePath("/p0/p1/p2?a=x&a=y&b=z");
  expect(parsedPath.path).toEqual("/p0/p1/p2");
  const q = parsedPath.query;
  expect(q.getAll("a")).toEqual(["x", "y"]);
  expect(q.get("b")).toEqual("z");
});

test("db functions", async () => {
  type Args = {
    query: string;
    params: unknown[];
  };
  let queryArgs: Args = { query: "", params: [] };
  let executeArgs: Args = { query: "", params: [] };

  {
    const query = async (query: string, params: unknown[]) => queryArgs = { query, params };
    const execute = async (query: string, params: unknown[]) => executeArgs = { query, params };

    globalThis.rustyscript = {
      ...globalThis.rustyscript,
      async_functions: {
        query,
        execute,
      },
    };
  }

  const executeStr = "INSERT INTO table (col) VALUES (?1)";
  await execute(executeStr, ["test"]);
  expect(executeArgs.query).toEqual(executeStr);
  expect(executeArgs.params).toEqual(["test"]);

  const queryStr = "SELECT * FROM table WHERE col = ?1";
  await query(queryStr, ["test"]);
  expect(queryArgs.query).toEqual(queryStr);
  expect(queryArgs.params).toEqual(["test"]);
});

test("routes functions", async () => {
  const promise = new Promise<StringRequestType>((resolve) => {
    addRoute("GET", "/test", stringHandler(async (req: StringRequestType) => {
      resolve(req);
      return "response";
    }));
  });

  const uri = "http://127.0.0.1";
  dispatch("GET", "/test", uri, [], [], undefined, encodeFallback("test"));

  const result: StringRequestType = await promise;

  expect(result.uri).toEqual(uri);
});

test("string handler", async () => {
  const req = {
    uri: "http://test.gov",
    params: {},
    headers: {},
  } satisfies RequestType;

  {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    const handler: CallbackType = stringHandler((_req) => "test");
    const response = (await handler(req))!;
    expect(decodeFallback(response.body!)).toEqual("test");
  }

  {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    const handler: CallbackType = stringHandler((_req) => {
      throw new HttpError(418);
    });
    expect((await handler(req))!.status).toEqual(418);
  }

  {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    const handler: CallbackType = htmlHandler((_req) => {
      throw new HttpError(418);
    });
    expect((await handler(req))!.status).toEqual(418);
  }

  {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    const handler: CallbackType = jsonHandler((_req) => {
      throw new HttpError(418);
    });
    expect((await handler(req))!.status).toEqual(418);
  }
});
