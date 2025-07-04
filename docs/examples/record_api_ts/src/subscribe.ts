import type { Client } from "trailbase";

export const subscribe = async (client: Client, id: string | number) =>
  await client.records("simple_strict_table").subscribe(id);

export const subscribeAll = async (client: Client) =>
  await client.records("simple_strict_table").subscribe("*");
