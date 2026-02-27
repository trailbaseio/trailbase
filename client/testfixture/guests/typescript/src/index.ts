import { defineConfig } from "trailbase-wasm";
import {
  HttpError,
  HttpHandler,
  HttpRequest,
  HttpResponse,
  StatusCode,
} from "trailbase-wasm/http";
import { execute, query, Transaction } from "trailbase-wasm/db";

export default defineConfig({
  httpHandlers: [
    HttpHandler.get("/js/fibonacci", (req: HttpRequest): string => {
      const n = req.getQueryParam("n");
      return `${fibonacci(n ? parseInt(n) : 40)}\n`;
    }),
    HttpHandler.get("/js/json", jsonHandler),
    HttpHandler.post("/js/json", jsonHandler),
    HttpHandler.get("/js/fetch", async (req: HttpRequest): Promise<string> => {
      const url = req.getQueryParam("url");
      if (url) {
        return await (await fetch(url)).text();
      }
      throw new HttpError(StatusCode.BAD_REQUEST, `Missing ?url param`);
    }),
    HttpHandler.get("/js/error", () => {
      throw new HttpError(StatusCode.IM_A_TEAPOT, "I'm a teapot");
    }),
    HttpHandler.get("/js/await", async (req) => {
      const ms = req.getQueryParam("ms");
      await delay(ms ? parseInt(ms) : 10);

      // Bodies over 2kB/4kB are streamed.
      return "A".repeat(5000);
    }),
    HttpHandler.get("/js/addDeletePost", async () => {
      const userId = (
        await query("SELECT id FROM _user WHERE email = 'admin@localhost'", [])
      )[0][0];

      console.info("[print from WASM JS guest] user id:", userId);

      const body = `${Date.now()} - ${Math.random()}`;
      const numInsertions = await execute(
        `INSERT INTO post (author, title, body) VALUES (?1, 'title' , ?2)`,
        [userId, body],
      );

      const numDeletions = await execute(`DELETE FROM post WHERE body = ?1`, [
        body,
      ]);

      return numInsertions === numDeletions ? "Ok" : "Fail";
    }),
    HttpHandler.get("/js/transaction", async () => {
      const tx = new Transaction();

      tx.execute("CREATE TABLE IF NOT EXISTS tx (id INTEGER PRIMARY KEY)", []);

      const rows = tx.query("SELECT COUNT(*) FROM tx", []);
      const count = rows[0][0] as bigint;
      console.assert(count >= 0);

      const rowsAffected = tx.execute("INSERT INTO tx (id) VALUES (?1)", [
        Number(count) + 1,
      ]);
      console.assert(rowsAffected == 1);

      tx.commit();

      return "Ok";
    }),
    HttpHandler.get("/js/set_interval", async (): Promise<string> => {
      var cnt = 0;

      console.log(`Registering callback`);
      const handle = setInterval(() => {
        console.log(`Interval: ${cnt}`);
        cnt += 1;

        if (cnt > 10) {
          clearInterval(handle);
        }
      }, 300);

      return `setInterval from Javascript`;
    }),
    HttpHandler.get("/js/random", async (): Promise<string> => {
      return `${Math.random().toString()}\n`;
    }),
  ],
});

function jsonHandler(req: HttpRequest): HttpResponse {
  const json = req.json();
  return HttpResponse.json(
    json ?? {
      int: 5,
      real: 4.2,
      msg: "foo",
      obj: {
        nested: true,
      },
    },
  );
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function fibonacci(num: number): number {
  switch (num) {
    case 0:
      return 0;
    case 1:
      return 1;
    default:
      return fibonacci(num - 1) + fibonacci(num - 2);
  }
}
