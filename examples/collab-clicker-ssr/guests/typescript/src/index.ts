import { Request, defineConfig, query } from "trailbase-wasm";

// @ts-ignore
import { render } from "../../../dist/server/entry-server.js";

// NOTE: Should we read this via fs APIs rather than baking it into the js?
import template from "../../../dist/client/index.html?raw";

async function clicked(_: Request): Promise<string> {
  const rows = await query(
    "UPDATE counter SET value = value + 1 WHERE id = 1 RETURNING value",
    [],
  );

  const count = rows.length > 0 ? (rows[0][0] as number) : -1;
  return JSON.stringify({ count });
}

async function ssr(_: Request): Promise<string> {
  const rows = await query("SELECT value FROM counter WHERE id = 1", []);

  const count = rows.length > 0 ? (rows[0][0] as number) : 0;
  const rendered = render("ignored", count);

  const html = template
    .replace(`<!--app-head-->`, rendered.head ?? "")
    .replace(`<!--app-html-->`, rendered.html ?? "")
    .replace(`<!--app-data-->`, rendered.data ?? "");

  return html;
}

export default defineConfig({
  httpHandlers: [
    {
      path: "/clicked",
      method: "get",
      handler: clicked,
    },
    {
      path: "/",
      method: "get",
      handler: ssr,
    },
  ],
});
