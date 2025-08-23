/* eslint-disable @typescript-eslint/no-unused-expressions */
import { expect, test } from "vitest";
import { initClient, urlSafeBase64Encode } from "../../src/index";
import type { Client, Event } from "../../src/index";
import { status } from "http-status";
import { v7 as uuidv7, parse as uuidParse } from "uuid";
import { ADDRESS } from "../constants";

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

type SimpleStrict = {
  id: string;

  text_null?: string;
  text_default?: string;
  text_not_null: string;

  // Add or generate missing fields.
};

type NewSimpleStrict = Partial<SimpleStrict>;

type SimpleCompleteView = SimpleStrict;

type SimpleSubsetView = {
  id: string;

  t_null?: string;
  t_default?: string;
  t_not_null: string;
};

async function connect(): Promise<Client> {
  const client = initClient(`http://${ADDRESS}`);
  await client.login("admin@localhost", "secret");
  return client;
}

// WARN: this test is not hermetic. I requires an appropriate TrailBase instance to be running.
test("auth integration tests", async () => {
  const client = await connect();

  const oldTokens = client.tokens();
  expect(oldTokens).not.undefined;

  // We need to wait a little to push the expiry time in seconds to avoid just getting the same token minted again.
  await sleep(1500);

  await client.refreshAuthToken();
  const newTokens = client.tokens();
  expect(newTokens).not.undefined.and.not.equals(oldTokens!.auth_token);

  const headers0 = client.headers();
  expect(headers0["Content-Type"]).toBeUndefined();
  expect(headers0["Authorization"].startsWith("Bearer ")).toBe(true);

  expect(await client.logout()).toBe(true);
  expect(client.user()).toBe(undefined);

  const headers1 = client.headers();

  expect(headers1["Authorization"]).toBeUndefined();
});

test("Record integration tests", async () => {
  const client = await connect();
  const api = client.records<NewSimpleStrict>("simple_strict_table");

  const now = new Date().getTime();
  // Throw in some url characters for good measure.
  const messages = [
    `ts client test 1: =?&${now}`,
    `ts client test 2: =?&${now}`,
  ];

  const ids: string[] = [];
  for (const msg of messages) {
    ids.push((await api.create({ text_not_null: msg })) as string);
  }

  {
    const bulkIds = await api.createBulk([
      { text_not_null: "ts bulk create 0" },
      { text_not_null: "ts bulk create 1" },
    ]);
    expect(bulkIds.length).toBe(2);
  }

  {
    const response = await api.list({
      filters: [
        {
          column: "text_not_null",
          value: messages[0],
        },
      ],
    });
    expect(response.total_count).toBeUndefined();
    expect(response.cursor).not.undefined.and.not.toBe("");
    const records = response.records;
    expect(records.length).toBe(1);
    expect(records[0].text_not_null).toBe(messages[0]);
  }

  {
    const response = await api.list({
      filters: [
        {
          column: "text_not_null",
          op: "like",
          value: `% =?&${now}`,
        },
      ],
      order: ["+text_not_null"],
      count: true,
    });
    expect(response.total_count).toBe(2);
    expect(response.records.map((el) => el.text_not_null)).toStrictEqual(
      messages,
    );
  }

  {
    const response = await api.list<SimpleStrict>({
      filters: [
        {
          column: "text_not_null",
          op: "like",
          value: `%${now}`,
        },
      ],
      order: ["-text_not_null"],
    });
    expect(
      response.records.map((el) => el.text_not_null).reverse(),
    ).toStrictEqual(messages);
  }

  const record: SimpleStrict = await api.read(ids[0]);
  expect(record.id).toStrictEqual(ids[0]);
  expect(record.text_not_null).toStrictEqual(messages[0]);

  // Test 1:1 view-bases record API.
  const view_record: SimpleCompleteView = await client
    .records("simple_complete_view")
    .read(ids[0]);
  expect(view_record.id).toStrictEqual(ids[0]);
  expect(view_record.text_not_null).toStrictEqual(messages[0]);

  // Test view-based record API with column renames.
  const subset_view_record: SimpleSubsetView = await client
    .records("simple_subset_view")
    .read(ids[0]);
  expect(subset_view_record.id).toStrictEqual(ids[0]);
  expect(subset_view_record.t_not_null).toStrictEqual(messages[0]);

  const updated_value: Partial<SimpleStrict> = {
    text_not_null: "updated not null",
    text_default: "updated default",
    text_null: "updated null",
  };
  await api.update(ids[1], updated_value);
  const updated_record: SimpleStrict = await api.read(ids[1]);
  expect(updated_record).toEqual(
    expect.objectContaining({
      id: ids[1],
      ...updated_value,
    }),
  );

  await api.delete(ids[1]);

  expect(await client.logout()).toBe(true);
  expect(client.user()).toBe(undefined);

  await expect(
    async () => await api.read<SimpleStrict>(ids[0]),
  ).rejects.toThrowError(
    expect.objectContaining({
      status: status.FORBIDDEN,
    }),
  );
});

type Comment = {
  id: number;
  body: string;
  post: {
    id: string;
    data?: {
      id: string;
      author: string;
      title: string;
      body: string;
    };
  };
  author: {
    id: string;
    data?: {
      id: string;
      user: string;
      name: string;
    };
  };
};

test("expand foreign records", async () => {
  const client = await connect();
  const api = client.records("comment");

  {
    const comment = await api.read<Comment>(1);
    expect(comment.id).toBe(1);
    expect(comment.body).toBe("first comment");
    expect(comment.author.data).toBeUndefined();
    expect(comment.post.data).toBeUndefined();
  }

  {
    const comment = await api.read<Comment>(1, { expand: ["post"] });
    expect(comment.id).toBe(1);
    expect(comment.body).toBe("first comment");
    expect(comment.author.data).toBeUndefined();
    expect(comment.post.data?.title).toBe("first post");
  }

  {
    const response = await api.list<Comment>({
      expand: ["author", "post"],
      order: ["-id"],
      pagination: {
        limit: 1,
      },
    });

    expect(response.records.length).toBe(1);
    const comment = response.records[0];

    expect(comment.id).toBe(2);
    expect(comment.body).toBe("second comment");
    expect(comment.author.data?.name).toBe("SecondUser");
    expect(comment.post.data?.title).toBe("first post");
  }

  {
    const response = await api.list<Comment>({
      expand: ["author", "post"],
      order: ["-id"],
      pagination: {
        limit: 2,
      },
    });

    expect(response.records.length).toBe(2);
    const second = response.records[1];

    const offsetResponse = await api.list<Comment>({
      expand: ["author", "post"],
      order: ["-id"],
      pagination: {
        limit: 1,
        offset: 1,
      },
    });

    expect(offsetResponse.records.length).toBe(1);
    const offsetFirst = offsetResponse.records[0];

    expect(second).toStrictEqual(offsetFirst);
  }
});

test("record error tests", async () => {
  const client = await connect();

  const nonExistantId = urlSafeBase64Encode(
    String.fromCharCode.apply(null, uuidParse(uuidv7())),
  );
  const nonExistantApi = client.records("non-existant");
  await expect(
    async () => await nonExistantApi.read<SimpleStrict>(nonExistantId),
  ).rejects.toThrowError(
    expect.objectContaining({
      status: status.METHOD_NOT_ALLOWED,
    }),
  );

  const api = client.records("simple_strict_table");
  await expect(
    async () => await api.read<SimpleStrict>("invalid id"),
  ).rejects.toThrowError(
    expect.objectContaining({
      status: status.BAD_REQUEST,
    }),
  );
  await expect(
    async () => await api.read<SimpleStrict>(nonExistantId),
  ).rejects.toThrowError(
    expect.objectContaining({
      status: status.NOT_FOUND,
    }),
  );
});

test("realtime subscribe specific record tests", async () => {
  const client = await connect();
  const api = client.records("simple_strict_table");

  const now = new Date().getTime();
  const createMessage = `ts client realtime test 0: =?&${now}`;
  const id = (await api.create<NewSimpleStrict>({
    text_not_null: createMessage,
  })) as string;

  const eventStream = await api.subscribe(id);

  const updatedMessage = `ts client updated realtime test 0: ${now}`;
  const updatedValue: Partial<SimpleStrict> = {
    text_not_null: updatedMessage,
  };
  await api.update(id, updatedValue);
  await api.delete(id);

  const events: Event[] = [];
  for await (const event of eventStream) {
    events.push(event);
  }

  expect(events).toHaveLength(2);
  expect(events[0]["Update"]["text_not_null"]).equals(updatedMessage);
  expect(events[1]["Delete"]["text_not_null"]).equals(updatedMessage);
});

test("transaction tests", async () => {
  const client = await connect();
  const now = new Date().getTime();

  // Test transaction with create operation
  {
    const batch = client.transaction();
    const record = { text_not_null: `ts transaction create test: =?&${now}` };
    batch.api("simple_strict_table").create(record);

    const ids = await batch.send();
    expect(ids).toHaveLength(1);

    // Verify record was created
    const api = client.records("simple_strict_table");
    const createdRecord = await api.read(ids[0]);
    expect(createdRecord.text_not_null).toBe(record.text_not_null);
  }

  // Test transaction with update operation
  {
    const api = client.records("simple_strict_table");
    const record = {
      text_not_null: `ts transaction update test original: =?&${now}`,
    };
    const id = await api.create(record);

    const batch = client.transaction();
    const updatedRecord = {
      text_not_null: `ts transaction update test modified: =?&${now}`,
    };
    batch.api("simple_strict_table").update(id, updatedRecord);

    await batch.send();
    const readRecord = await api.read(id);
    expect(readRecord.text_not_null).toBe(updatedRecord.text_not_null);
  }

  // Test transaction with delete operation
  {
    const api = client.records("simple_strict_table");
    const record = { text_not_null: `ts transaction delete test: =?&${now}` };
    const id = await api.create(record);

    const batch = client.transaction();
    batch.api("simple_strict_table").delete(id);

    await batch.send();
    await expect(api.read(id)).rejects.toThrow();
  }
});

test("realtime subscribe table tests", async () => {
  const client = await connect();
  const api = client.records("simple_strict_table");
  const eventStream = await api.subscribe("*");

  const now = new Date().getTime();
  const createMessage = `ts client realtime test 0: =?&${now}`;
  const id = (await api.create<NewSimpleStrict>({
    text_not_null: createMessage,
  })) as string;

  const updatedMessage = `ts client updated realtime test 0: ${now}`;
  const updatedValue: Partial<SimpleStrict> = {
    text_not_null: updatedMessage,
  };
  await api.update(id, updatedValue);
  await api.delete(id);

  const events: Event[] = [];
  for await (const event of eventStream) {
    events.push(event);

    if (events.length === 3) {
      break;
    }
  }

  expect(events).toHaveLength(3);
  expect(events[0]["Insert"]["text_not_null"]).equals(createMessage);
  expect(events[1]["Update"]["text_not_null"]).equals(updatedMessage);
  expect(events[2]["Delete"]["text_not_null"]).equals(updatedMessage);
});
