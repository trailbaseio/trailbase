import { defineConfig } from "trailbase-wasm";
import { query } from "trailbase-wasm/db";
import { HttpHandler, HttpResponse, StatusCode } from "trailbase-wasm/http";
import type { HttpRequest } from "trailbase-wasm/http";

async function countRecordsHandler(req: HttpRequest): Promise<HttpResponse> {
  const table = req.getPathParam("table");
  if (!table) {
    return HttpResponse.status(
      StatusCode.BAD_REQUEST,
      `Table not found for '?table=${table}'`,
    );
  }

  const rows = await query(`SELECT COUNT(*) FROM ${table}`, []);
  return HttpResponse.text(`count: ${rows[0][0]}`);
}

export default defineConfig({
  httpHandlers: [HttpHandler.get("/count/{table}", countRecordsHandler)],
});
