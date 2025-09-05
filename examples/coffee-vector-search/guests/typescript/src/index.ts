import { Request, HttpHandler, defineConfig } from "trailbase-wasm";
import { query } from "trailbase-wasm/db";

async function searchHandler(req: Request): Promise<string> {
  // Get the query params from the url, e.g. '/search?aroma=4&acidity=7'.
  const aroma = req.getQueryParam("aroma") ?? 8;
  const flavor = req.getQueryParam("flavor") ?? 8;
  const acid = req.getQueryParam("acidity") ?? 8;
  const sweet = req.getQueryParam("sweetness") ?? 8;

  // Query the database for the closest match.
  const rows = await query(
    `SELECT Owner, Aroma, Flavor, Acidity, Sweetness
         FROM coffee
         ORDER BY vec_distance_L2(
           embedding, FORMAT("[%f, %f, %f, %f]", $1, $2, $3, $4))
         LIMIT 100`,
    [+aroma, +flavor, +acid, +sweet],
  );

  return JSON.stringify(rows);
}

export default defineConfig({
  httpHandlers: [
    HttpHandler.get("/search", searchHandler),
  ],
});
