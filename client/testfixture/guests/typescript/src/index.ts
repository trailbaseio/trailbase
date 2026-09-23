import { defineConfig } from "trailbase-wasm";
import {
  HttpError,
  HttpHandler,
  HttpRequest,
  HttpResponse,
  StatusCode,
} from "trailbase-wasm/http";
import { execute, executeBatch, query, Transaction } from "trailbase-wasm/db";

//const PREFIX = "";
const PREFIX = "/js";

export const { initEndpoint, incomingHandler, sqliteFunctionEndpoint } =
  defineConfig({
    metadata: {
      display_name: "TestFixture TypeScript",
      description: "A component used within Trailbase's tests.",
      admin_ui_path: `${PREFIX}/dash`,
    },
    httpHandlers: [
      HttpHandler.get(`${PREFIX}/rt`, (_: HttpRequest): string => "JS"),
      HttpHandler.get(`${PREFIX}/method`, (_: HttpRequest): string => "get"),
      HttpHandler.post(`${PREFIX}/method`, (_: HttpRequest): string => "post"),
      HttpHandler.delete(
        `${PREFIX}/method`,
        (_: HttpRequest): string => "delete",
      ),
      HttpHandler.get(`${PREFIX}/fibonacci`, (req: HttpRequest): string => {
        const n = req.getQueryParam("n");
        return `${fibonacci(n ? parseInt(n) : 40)}\n`;
      }),
      HttpHandler.get(`${PREFIX}/json`, jsonHandler),
      HttpHandler.post(`${PREFIX}/json`, jsonHandler),
      HttpHandler.get(
        `${PREFIX}/fetch`,
        async (req: HttpRequest): Promise<string> => {
          const url = req.getQueryParam("url");
          if (url) {
            return await (await fetch(url)).text();
          }
          throw new HttpError(StatusCode.BAD_REQUEST, `Missing ?url param`);
        },
      ),
      HttpHandler.get(`${PREFIX}/error`, () => {
        throw new HttpError(StatusCode.IM_A_TEAPOT, "I'm a teapot");
      }),
      HttpHandler.get(`${PREFIX}/panic`, () => {
        throw `some error`;
      }),
      HttpHandler.get(`${PREFIX}/await`, async (req) => {
        const ms = req.getQueryParam("ms");
        await delay(ms ? parseInt(ms) : 10);

        // Bodies over 2kB/4kB are streamed.
        return "A".repeat(5000);
      }),
      HttpHandler.get(`${PREFIX}/addDeletePost`, async () => {
        const user = "admin@localhost";
        const userId = (
          await query("SELECT id FROM _user WHERE email = ?1", [user])
        )[0][0];

        console.info(
          `[print from WASM JS guest] user id of '${user}':`,
          btoa(String.fromCharCode(...(userId as Uint8Array))),
        );

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
      HttpHandler.get(`${PREFIX}/transaction`, async () => {
        const tx = new Transaction();

        tx.execute(
          "CREATE TABLE IF NOT EXISTS tx (id INTEGER PRIMARY KEY)",
          [],
        );

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
      HttpHandler.get(`${PREFIX}/query_db/{sql}`, async (req: HttpRequest) => {
        const sql = atob(req.getPathParam("sql") ?? "");
        const rows = await query(sql, []);
        return `${rows[0][0]}`;
      }),
      HttpHandler.get(
        `${PREFIX}/execute_db/{sql}`,
        async (req: HttpRequest) => {
          const sql = atob(req.getPathParam("sql") ?? "");
          const rowsAffected = await execute(sql, []);
          return `${rowsAffected}`;
        },
      ),
      HttpHandler.get(
        `${PREFIX}/execute_batch_db/{sql}`,
        async (req: HttpRequest) => {
          const sql = atob(req.getPathParam("sql") ?? "");
          await executeBatch(sql);
        },
      ),
      HttpHandler.get(`${PREFIX}/set_interval`, async (): Promise<string> => {
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
      HttpHandler.get(`${PREFIX}/random`, async (): Promise<string> => {
        return `${Math.random().toString()}\n`;
      }),
      // Dashboard:
      HttpHandler.get(`${PREFIX}/dash`, (_: HttpRequest): string => dash),
      // Built-in SQLite extension:
      HttpHandler.get(
        `${PREFIX}/test_sqlite-vec`,
        async (): Promise<string> => {
          const vec = (await query("SELECT vec_f32('[0, 1, 2, 3]')", []))[0][0];

          return btoa(String.fromCharCode(...(vec as Uint8Array)));
        },
      ),
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

const dash = `
<html>
<body style="background-color:#92a8d1;" >
  <h1>Testfixture Dash</h1>

  <p>
    Greetings from TypeScript.
  </p>
</body>
</html>
`;
