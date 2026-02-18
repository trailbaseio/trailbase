import { defineConfig, addPeriodicCallback } from "trailbase-wasm";
import { HttpHandler, HttpRequest, HttpResponse } from "trailbase-wasm/http";
import { JobHandler } from "trailbase-wasm/job";
import { query } from "trailbase-wasm/db";

export default defineConfig({
  httpHandlers: [
    HttpHandler.get("/fibonacci", (req: HttpRequest) => {
      const n = req.getQueryParam("n");
      return fibonacci(n ? parseInt(n) : 40).toString();
    }),
    HttpHandler.get("/json", jsonHandler),
    HttpHandler.post("/json", jsonHandler),
    HttpHandler.get("/a", (req: HttpRequest) => {
      const n = req.getQueryParam("n");
      return "a".repeat(n ? parseInt(n) : 5000);
    }),
    HttpHandler.get("/interval", () => {
      let i = 0;
      addPeriodicCallback(250, (cancel) => {
        console.log(`callback #${i}`);
        i += 1;
        if (i >= 10) {
          cancel();
        }
      });
    }),
    HttpHandler.get("/sleep", async (req: HttpRequest) => {
      const param = req.getQueryParam("ms");
      const ms: number = param ? parseInt(param) : 500;
      await sleep(ms);
      return `slept: ${ms}ms`;
    }),
    HttpHandler.get("/count/{table}/", async (req: HttpRequest) => {
      const table = req.getPathParam("table");
      if (table) {
        const rows = await query(`SELECT COUNT(*) FROM ${table}`, []);
        return `Got ${rows[0][0]} rows\n`;
      }
    }),
  ],
  jobHandlers: [JobHandler.minutely("myjob", () => console.log("Hello Job!"))],
});

function jsonHandler(req: HttpRequest) {
  return HttpResponse.json(
    req.json() ?? {
      int: 5,
      real: 4.2,
      msg: "foo",
      obj: {
        nested: true,
      },
    },
  );
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

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
