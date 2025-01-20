import { Client } from "trailbase";

export const create = async (client: Client): Promise<string | number> =>
  await client.records("simple_strict_table").createId({ text_not_null: "test" });
