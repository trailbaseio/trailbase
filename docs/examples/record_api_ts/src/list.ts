import type { Client, ListResponse } from "trailbase";

export const list = async (client: Client): Promise<ListResponse<object>> =>
  await client.records("movies").list({
    pagination: {
      limit: 3,
    },
    order: ["rank"],
    filters: [
      // Multiple filters on same column: watch_time between 90 and 120 minutes
      {
        column: "watch_time",
        op: "greaterThanOrEqual",
        value: "90",
      },
      {
        column: "watch_time",
        op: "lessThan",
        value: "120",
      },
      // Date range: movies released between 2020 and 2023
      {
        column: "release_date",
        op: "greaterThanOrEqual",
        value: "2020-01-01",
      },
      {
        column: "release_date",
        op: "lessThanOrEqual",
        value: "2023-12-31",
      },
    ],
  });
