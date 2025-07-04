import type { Client, ListResponse } from "trailbase";

export const list = async (client: Client): Promise<ListResponse<object>> =>
  await client.records("movies").list({
    pagination: {
      limit: 3,
    },
    order: ["rank"],
    filters: [
      {
        column: "watch_time",
        op: "lessThan",
        value: "120",
      },
      {
        column: "description",
        op: "like",
        value: "%love%",
      },
    ],
  });
