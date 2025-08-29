import {
  addCronCallback,
  addPeriodicCallback,
  addRoute,
  execute,
  htmlHandler,
  jsonHandler,
  parsePath,
  query,
  stringHandler,
  transaction,
  HttpError,
  StatusCodes,
  Transaction,
} from "../trailbase.js";
import type {
  Blob,
  JsonRequestType,
  ParsedPath,
  StringRequestType,
} from "../trailbase.d.ts";

addRoute(
  "GET",
  "/test",
  stringHandler(async (req: StringRequestType) => {
    const uri: ParsedPath = parsePath(req.uri);

    const table = uri.query.get("table");
    if (table) {
      const rows = await query(`SELECT COUNT(*) FROM "${table}"`, []);
      return `entries: ${rows[0][0]}`;
    }

    return `test: ${req.uri}`;
  }),
);

addRoute(
  "GET",
  "/test/{table}",
  stringHandler(async (req: StringRequestType) => {
    const table = req.params["table"];
    if (table) {
      const rows = await query(`SELECT COUNT(*) FROM "${table}"`, []);
      return `entries: ${rows[0][0]}`;
    }

    return `test: ${req.uri}`;
  }),
);

addRoute(
  "GET",
  "/tx/{table}",
  stringHandler(async (req: StringRequestType) => {
    const table = req.params["table"];
    if (table) {
      const count = await transaction((tx: Transaction) => {
        const rows = tx.query(`SELECT COUNT(*) FROM "${table}"`, []);
        return rows[0][0] as number;
      });

      return `entries: ${count}`;
    }

    return `test: ${req.uri}`;
  }),
);

addRoute(
  "GET",
  "/html",
  htmlHandler((_req: StringRequestType) => {
    return `
    <html>
      <body>
        <h1>Html Handler</h1>
      </body>
    </html>
  `;
  }),
);

addRoute(
  "GET",
  "/json",
  jsonHandler((_req: JsonRequestType) => {
    return {
      int: 5,
      real: 4.2,
      msg: "foo",
      obj: {
        nested: true,
      },
    };
  }),
);

addRoute(
  "GET",
  "/error",
  jsonHandler((_req: JsonRequestType) => {
    throw new HttpError(StatusCodes.IM_A_TEAPOT, "I'm a teapot");
  }),
);

addRoute(
  "GET",
  "/fetch",
  stringHandler(async (req: StringRequestType) => {
    const query = parsePath(req.uri).query;
    const url = query.get("url");

    if (url) {
      const response = await fetch(url);
      return await response.text();
    }

    throw new HttpError(StatusCodes.BAD_REQUEST, `Missing ?url param: ${req.params}`);
  }),
);

addRoute(
  "GET",
  "/addDeletePost",
  stringHandler(async (_req: StringRequestType) => {
    const userId: Blob = (
      await query("SELECT id FROM _user WHERE email = 'admin@localhost'", [])
    )[0][0] as Blob;

    console.info("user id:", userId.blob);

    const now = Date.now().toString();
    const numInsertions = await execute(
      `INSERT INTO post (author, title, body) VALUES (?1, 'title' , ?2)`,
      [{ blob: userId.blob }, now],
    );

    const numDeletions = await execute(`DELETE FROM post WHERE body = ?1`, [
      now,
    ]);

    console.assert(numInsertions == numDeletions);

    return "Ok";
  }),
);

class Completer<T> {
  public readonly promise: Promise<T>;
  public complete: (value: PromiseLike<T> | T) => void;

  public constructor() {
    this.promise = new Promise<T>((resolve, _reject) => {
      this.complete = resolve;
    });
  }
}

const completer = new Completer<string>();

addPeriodicCallback(100, (cancel) => {
  completer.complete("resolved");
  cancel();
});

addCronCallback("JS-registered Job", "@hourly", async () => {
  console.info("JS-registered cron job reporting for duty 🚀");
});

addRoute(
  "GET",
  "/await",
  stringHandler(async (_req) => await completer.promise),
);
