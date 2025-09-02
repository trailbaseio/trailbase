import {
  HttpError,
  HttpHandler,
  OutgoingResponse,
  Request,
  StatusCode,
  buildJsonResponse,
  defineConfig,
  execute,
  query,
} from "trailbase-wasm";

export default defineConfig({
  httpHandlers: [
    HttpHandler.get("/fibonacci", fibonacciHandler),
    HttpHandler.get("/json", (_: Request): OutgoingResponse => {
      return buildJsonResponse({
        int: 5,
        real: 4.2,
        msg: "foo",
        obj: {
          nested: true,
        },
      });
    }),
    HttpHandler.get("/fetch", async (req: Request): Promise<string> => {
      const url = req.getQueryParam("url");
      if (url) {
        return await (await fetch(url)).text();
      }
      throw new HttpError(
        StatusCode.BAD_REQUEST,
        `Missing ?url param: ${req.params}`,
      );
    }),
    HttpHandler.get("/error", () => {
      throw new HttpError(StatusCode.IM_A_TEAPOT, "I'm a teapot");
    }),
    HttpHandler.get("/await", async (req) => {
      const ms = req.getQueryParam("ms");
      await delay(ms ? parseInt(ms) : 10);

      // Bodies over 2kB/4kB are streamed.
      return "A".repeat(5000);
    }),
    HttpHandler.get("/addDeletePost", async () => {
      const userId = (
        await query("SELECT id FROM _user WHERE email = 'admin@localhost'", [])
      )[0][0];

      console.info("user id:", userId);

      const now = Date.now().toString();
      const numInsertions = await execute(
        `INSERT INTO post (author, title, body) VALUES (?1, 'title' , ?2)`,
        [userId, now],
      );

      const numDeletions = await execute(`DELETE FROM post WHERE body = ?1`, [
        now,
      ]);

      console.assert(numInsertions === numDeletions);

      return "Ok";
    }),
    HttpHandler.get("/set_interval", async (_req: Request): string => {
      var cnt = 0;

      console.log(`Registering callback`);
      const handle = setInterval(() => {
        console.log(`Interval: ${cnt}`);
        cnt += 1;

        if (cnt > 10) {
          clearInterval(handle);
        }
      }, 300);

      await delay(700);

      return `setInterval from Javascript`;
    }),
  ],
});

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function fibonacciHandler(req: Request): string {
  const n = req.getQueryParam("n");
  return `${fibonacci(n ? parseInt(n) : 40)}\n`;
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
