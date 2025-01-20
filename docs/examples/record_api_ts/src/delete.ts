import { Client } from "trailbase";

export const remove = async (client: Client, id: string | number) =>
  await client.records("simple_strict_table").delete(id);
