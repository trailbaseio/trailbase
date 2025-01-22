import { Client, type Event } from "trailbase";
import { expect, test } from "vitest";

import { create } from "../src/create.ts";
import { read } from "../src/read.ts";
import { update } from "../src/update.ts";
import { remove } from "../src/delete.ts";
import { list } from "../src/list.ts";
import { subscribe, subscribeAll } from "../src/subscribe.ts";

async function connect(): Promise<Client> {
  const client = new Client("http://localhost:4000");
  await client.login("admin@localhost", "secret");
  return client;
}

test("Test code examples", async () => {
  const client = await connect();

  const tableStream = await subscribeAll(client);

  const id = await create(client);

  const recordStream = await subscribe(client, id);

  {
    const record = await read(client, id);
    expect(record).toMatchObject({ "text_not_null": "test" });
  }

  {
    await update(client, id, { "text_not_null": "updated" });
    const record = await read(client, id);
    expect(record).toMatchObject({ "text_not_null": "updated" });
  }

  await remove(client, id);

  {
    const events: Event[] = [];
    for await (const event of tableStream) {
      events.push(event);
      if (events.length === 3) {
        break;
      }
    }
    tableStream.cancel();
  }

  {
    const events: Event[] = [];
    for await (const event of recordStream) {
      events.push(event);
      if (events.length === 2) {
        break;
      }
    }
    recordStream.cancel();
  }

});

test("Test list examples", async () => {
  const client = await connect();

  const response = await list(client);

  expect(response.records.length).toBe(3);

  type Movie = {
    name: string;
  };

  const record = response.records[0] as Movie;
  expect(record.name).toBe("Casablanca");
});
