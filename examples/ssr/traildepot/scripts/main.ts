import { addRoute, htmlHandler, jsonHandler, query, fs } from "../trailbase.js";
import { render } from "./entry-server.js";

let _template: Promise<string> | null = null;

async function getTemplate(): Promise<string> {
  if (_template == null) {
    const template = _template = fs.readTextFile('solid-ssr-app/dist/client/index.html');
    return await template;
  }
  return await _template;
}

addRoute(
  "GET",
  "/clicked",
  jsonHandler(async (_req) => {
    const rows = await query(
      "UPDATE counter SET value = value + 1 WHERE id = 1 RETURNING value",
      [],
    )

    const count = rows.length > 0 ? rows[0][0] as number : -1;
    return { count };
  }),
);

/// Register a root handler.
addRoute(
  "GET",
  "/",
  htmlHandler(async (req) => {
    // NOTE: this is replicating vite SSR template's server.js;
    const rows = await query(
      "SELECT value FROM counter WHERE id = 1",
      [],
    )

    const count = rows.length > 0 ? rows[0][0] as number : 0;
    const rendered = render(req.uri, count);

    const html = (await getTemplate())
      .replace(`<!--app-head-->`, rendered.head ?? '')
      .replace(`<!--app-html-->`, rendered.html ?? '')
      .replace(`<!--app-data-->`, rendered.data ?? '');

    return html;
  }),
);
