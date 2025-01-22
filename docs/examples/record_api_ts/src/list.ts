import { Client, type ListResponse } from "trailbase";

export const list = async (client: Client): Promise<ListResponse<object>> =>
  await client.records("movies").list({
    pagination: {
      limit: 3,
    },
    order: ["rank"],
    filters: ["watch_time[lt]=120", "description[like]=%love%"],
  });
