import { defineConfig } from "trailbase-wasm";
import { HttpHandler, HttpRequest } from "trailbase-wasm/http";
import { query } from "trailbase-wasm/db";
import { readFileSync } from "trailbase-wasm/fs";
import { Store } from "trailbase-wasm/kv";

// @ts-expect-error: just trying to avoid aliases.
import { render } from "../../../dist/server/entry-server.js";

function readTemplate(): string {
  // NOTE: Since we're using vite, we could also bake the template rather than
  // reading it from the file-system?
  // import template from "../../../dist/client/index.html?raw";

  return new TextDecoder().decode(
    readCachedFileSync("/dist/client/index.html"),
  );
}

function readCachedFileSync(path: string): Uint8Array {
  const store = Store.open();

  const template = store.get(path);
  if (template !== undefined) {
    return template;
  }

  const contents = readFileSync(path);
  store.set(path, contents);
  return contents;
}

async function clicked(_req: HttpRequest): Promise<string> {
  const rows = await query(
    "UPDATE counter SET value = value + 1 WHERE id = 1 RETURNING value",
    [],
  );

  const count = rows.length > 0 ? (rows[0][0] as number) : -1;
  return JSON.stringify({ count });
}

async function ssr(req: HttpRequest): Promise<string> {
  const rows = await query("SELECT value FROM counter WHERE id = 1", []);

  const count = rows.length > 0 ? (rows[0][0] as number) : 0;
  const rendered = render(req.url(), count);

  const html = readTemplate()
    .replace(`<!--app-head-->`, rendered.head ?? "")
    .replace(`<!--app-html-->`, rendered.html ?? "")
    .replace(`<!--app-data-->`, rendered.data ?? "");

  return html;
}

export default defineConfig({
  httpHandlers: [
    HttpHandler.get("/clicked", clicked),
    HttpHandler.get("/", ssr),
  ],
});
