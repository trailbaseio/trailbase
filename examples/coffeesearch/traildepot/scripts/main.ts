import { addRoute, jsonHandler, parsePath, query } from "../trailbase.js";
import type { JsonRequestType } from "../trailbase.js";

addRoute(
  "GET",
  "/search",
  jsonHandler(async (req: JsonRequestType) => {
    const searchParams = parsePath(req.uri).query;

    const aroma = searchParams.get("aroma") ?? 8;
    const flavor = searchParams.get("flavor") ?? 8;
    const acidity = searchParams.get("acidity") ?? 8;
    const sweetness = searchParams.get("sweetness") ?? 8;

    return await query(
      `SELECT
         Owner,
         Aroma,
         Flavor,
         Acidity,
         Sweetness
       FROM
         coffee
       WHERE
         embedding IS NOT NULL
       ORDER BY
         vector_distance_cos(embedding, '[${aroma}, ${flavor}, ${acidity}, ${sweetness}]')
       LIMIT 100`,
      [],
    );
  }),
);
