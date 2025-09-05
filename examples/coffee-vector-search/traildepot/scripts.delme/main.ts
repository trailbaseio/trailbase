import { addRoute, jsonHandler, parsePath, query } from "../trailbase.js";

/// Register a handler for the `/search` API route.
addRoute(
  "GET",
  "/search",
  jsonHandler(async (req) => {
    // Get the query params from the url, e.g. '/search?aroma=4&acidity=7'.
    const searchParams = parsePath(req.uri).query;
    const aroma = searchParams.get("aroma") ?? 8;
    const flavor = searchParams.get("flavor") ?? 8;
    const acid = searchParams.get("acidity") ?? 8;
    const sweet = searchParams.get("sweetness") ?? 8;

    // Query the database for the closest match.
    return await query(
      `SELECT Owner, Aroma, Flavor, Acidity, Sweetness
         FROM coffee
         ORDER BY vec_distance_L2(
           embedding, FORMAT("[%f, %f, %f, %f]", $1, $2, $3, $4))
         LIMIT 100`,
      [+aroma, +flavor, +acid, +sweet],
    );
  }),
);
