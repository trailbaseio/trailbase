import type { Client } from "trailbase";

export const read = async (client: Client, id: string | number) =>
  await client.records("simple_strict_table").read(id);
