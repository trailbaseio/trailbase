import { Client } from "trailbase";

export const update = async (client: Client, id: string | number, record: object) =>
  await client.records("simple_strict_table").update(id, record);
