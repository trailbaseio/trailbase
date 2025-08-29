import {
  HttpError,
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
    {
      path: "/fibonacci",
      method: "get",
      handler: fibonacciHandler,
    },
    {
      path: "/json",
      method: "get",
      handler: (_: Request): OutgoingResponse => {
        return buildJsonResponse({
          int: 5,
          real: 4.2,
          msg: "foo",
          obj: {
            nested: true,
          },
        });
      },
    },
    {
      path: "/fetch",
      method: "get",
      handler: async (req: Request): Promise<string> => {
        const url = req.getQueryParam("url");
        if (url) {
          return await (await fetch(url)).text();
        }
        throw new HttpError(
          StatusCode.BAD_REQUEST,
          `Missing ?url param: ${req.params}`,
        );
      },
    },
    {
      path: "/error",
      method: "get",
      handler: () => {
        throw new HttpError(StatusCode.IM_A_TEAPOT, "I'm a teapot");
      },
    },
    {
      path: "/await",
      method: "get",
      handler: async () => {
        await delay(10);
        return "".repeat(5000);
      },
    },
    {
      path: "/addDeletePost",
      method: "get",
      handler: async () => {
        const userId = (
          await query(
            "SELECT id FROM _user WHERE email = 'admin@localhost'",
            [],
          )
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
      },
    },
  ],
});

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function fibonacciHandler(req: Request): string {
  const n = req.getQueryParam("n");
  return fibonacci(n ? parseInt(n) : 40).toString();
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
