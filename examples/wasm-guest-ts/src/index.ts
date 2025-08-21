import {
  Request,
  defineConfig,
  threadId,
  query,
  execute,
} from "trailbase-wasm";

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

export default defineConfig({
  httpHandlers: [
    {
      path: "/fibonacci",
      method: "get",
      handler: (_req: Request): string => {
        return fibonacci(40).toString();
      },
    },
    {
      path: "/sleep",
      method: "get",
      handler: async (_req: Request): Promise<string> => {
        await delay(10 * 1000);
        return "";
      },
    },
    {
      path: "/wasm/{placeholder}",
      method: "get",
      handler: (req: Request): string => {
        return `Hello from Javascript (${threadId()}): ${req.path}!\n`;
      },
    },
    {
      path: "/wasm_query",
      method: "get",
      handler: async (_req: Request): Promise<string> => {
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
      },
    },
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
