import { HttpHandler, Request, defineConfig, threadId } from "trailbase-wasm";
import { query, execute } from "trailbase-wasm/db";

export default defineConfig({
  httpHandlers: [
    HttpHandler.get("/fibonacci", (req: Request): string => {
      const n = req.getQueryParam("n");
      return fibonacci(n ? parseInt(n) : 40).toString();
    }),
    HttpHandler.get("/sleep", async (_req: Request): Promise<string> => {
      await delay(10 * 1000);
      return "A".repeat(2049);
    }),
    HttpHandler.get("/wasm/{placeholder}", (req: Request): string => {
      const param = req.getPathParam("placeholder");
      return `Hello from Javascript (${threadId()}): ${param}!\n`;
    }),
    HttpHandler.get("/wasm_query", async (_req: Request): Promise<string> => {
      await execute(
        "CREATE TABLE IF NOT EXISTS test (id INTEGER PRIMARY KEY)",
        [],
      );

      try {
        await execute("INSERT INTO test (id) VALUES (2), (4)", []);
      } catch (e) {
        console.error(`other: ${e}`);
      }

      const r = await query("SELECT COUNT(*) FROM test", []);
      const count = r[0][0] as number;
      return `Got ${count} rows\n`;
    }),
  ],
  jobHandlers: [
    {
      name: "mywasmjob",
      spec: "@hourly",
      handler: async () => {
        console.log("mywasmjob");
      },
    },
  ],
});

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

function delay(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
