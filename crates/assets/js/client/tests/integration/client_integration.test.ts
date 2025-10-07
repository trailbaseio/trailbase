/* eslint-disable @typescript-eslint/no-unused-expressions */
import { expect, test } from "vitest";
import {
  initClient,
  urlSafeBase64Encode,
  filePath,
  filesPath,
} from "../../src/index";
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
  const apiName = "simple_strict_table";
  const api = client.records<NewSimpleStrict>(apiName);

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
    const op: {
      Create: {
        api_name: string;
        value: Record<string, unknown>;
      };
    } = JSON.parse(JSON.stringify(api.createOp({ text_not_null: "test" })));

    expect(op.Create.api_name).toBe(apiName);
    expect(op.Create.value.text_not_null).toBe("test");

    const bulkIds = await client.execute(
      [
        api.createOp({ text_not_null: "ts bulk execute 0" }),
        api.createOp({ text_not_null: "ts bulk execute 1" }),
      ],
      false,
    );
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
    const response = await api.list({
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

  const record = await api.read(ids[0]);
  expect(record.id).toStrictEqual(ids[0]);
  expect(record.text_not_null).toStrictEqual(messages[0]);

  // Test 1:1 view-bases record API.
  const view_record: SimpleCompleteView = await client
    .records<SimpleCompleteView>("simple_complete_view")
    .read(ids[0]);
  expect(view_record.id).toStrictEqual(ids[0]);
  expect(view_record.text_not_null).toStrictEqual(messages[0]);

  // Test view-based record API with column renames.
  const subset_view_record: SimpleSubsetView = await client
    .records<SimpleSubsetView>("simple_subset_view")
    .read(ids[0]);
  expect(subset_view_record.id).toStrictEqual(ids[0]);
  expect(subset_view_record.t_not_null).toStrictEqual(messages[0]);

  const updated_value: Partial<SimpleStrict> = {
    text_not_null: "updated not null",
    text_default: "updated default",
    text_null: "updated null",
  };
  await api.update(ids[1], updated_value);
  const updated_record = await api.read(ids[1]);
  expect(updated_record).toEqual(
    expect.objectContaining({
      id: ids[1],
      ...updated_value,
    }),
  );

  await api.delete(ids[1]);

  expect(await client.logout()).toBe(true);
  expect(client.user()).toBe(undefined);

  await expect(async () => await api.read(ids[0])).rejects.toThrowError(
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
  const api = client.records<Comment>("comment");

  {
    const comment = await api.read(1);
    expect(comment.id).toBe(1);
    expect(comment.body).toBe("first comment");
    expect(comment.author.data).toBeUndefined();
    expect(comment.post.data).toBeUndefined();
  }

  {
    const comment = await api.read(1, { expand: ["post"] });
    expect(comment.id).toBe(1);
    expect(comment.body).toBe("first comment");
    expect(comment.author.data).toBeUndefined();
    expect(comment.post.data?.title).toBe("first post");
  }

  {
    const response = await api.list({
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
    const response = await api.list({
      expand: ["author", "post"],
      order: ["-id"],
      pagination: {
        limit: 2,
      },
    });

    expect(response.records.length).toBe(2);
    const second = response.records[1];

    const offsetResponse = await api.list({
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
    async () => await nonExistantApi.read(nonExistantId),
  ).rejects.toThrowError(
    expect.objectContaining({
      status: status.METHOD_NOT_ALLOWED,
    }),
  );

  const api = client.records("simple_strict_table");
  await expect(async () => await api.read("invalid id")).rejects.toThrowError(
    expect.objectContaining({
      status: status.BAD_REQUEST,
    }),
  );
  await expect(async () => await api.read(nonExistantId)).rejects.toThrowError(
    expect.objectContaining({
      status: status.NOT_FOUND,
    }),
  );
});

test("realtime subscribe specific record tests", async () => {
  const client = await connect();
  const api = client.records<NewSimpleStrict>("simple_strict_table");

  const now = new Date().getTime();
  const createMessage = `ts client realtime test 0: =?&${now}`;
  const id = (await api.create({
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
  const api = client.records<NewSimpleStrict>("simple_strict_table");
  const now = new Date().getTime();

  // Test transaction with create operation
  {
    const record = { text_not_null: `ts transaction create test: =?&${now}` };
    const ids = await client.execute([api.createOp(record)]);

    expect(ids).toHaveLength(1);

    // Verify record was created
    const createdRecord = await api.read(ids[0]);
    expect(createdRecord.text_not_null).toBe(record.text_not_null);
  }

  // Test transaction with update operation
  {
    const record = {
      text_not_null: `ts transaction update test original: =?&${now}`,
    };
    const id = await api.create(record);
    const updatedRecord = {
      text_not_null: `ts transaction update test modified: =?&${now}`,
    };
    await client.execute([api.updateOp(id, updatedRecord)]);

    const readRecord = await api.read(id);
    expect(readRecord.text_not_null).toBe(updatedRecord.text_not_null);
  }

  // Test transaction with delete operation
  {
    const record = { text_not_null: `ts transaction delete test: =?&${now}` };
    const id = await api.create(record);

    await client.execute([api.deleteOp(id)]);

    await expect(api.read(id)).rejects.toThrow();
  }
});

test("realtime subscribe table tests", async () => {
  const client = await connect();
  const api = client.records<NewSimpleStrict>("simple_strict_table");
  const eventStream = await api.subscribeAll();

  const now = new Date().getTime();
  const createMessage = `ts client realtime test 0: =?&${now}`;
  const id = (await api.create({
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

test("realtime subscribe to table with filters tests", async () => {
  const client = await connect();
  const api = client.records<NewSimpleStrict>("simple_strict_table");

  const now = new Date().getTime();

  const updatedMessage = `ts client updated realtime test 42: ${now}`;
  const eventStream = await api.subscribeAll({
    filters: [
      {
        column: "text_not_null",
        op: "equal",
        value: updatedMessage,
      },
    ],
  });

  const createMessage = `ts client realtime test 42: =?&${now}`;
  const id = (await api.create({
    text_not_null: createMessage,
  })) as string;

  const updatedValue: Partial<SimpleStrict> = {
    text_not_null: updatedMessage,
  };
  await api.update(id, updatedValue);
  await api.delete(id);

  const events: Event[] = [];
  for await (const event of eventStream) {
    events.push(event);

    if (events.length === 2) {
      break;
    }
  }

  // We should have skipped the creation.
  expect(events).toHaveLength(2);
  expect(events[0]["Update"]["text_not_null"]).equals(updatedMessage);
  expect(events[1]["Delete"]["text_not_null"]).equals(updatedMessage);
});

type FileUpload = {
  // Upload
  name?: string;
  data?: string;

  // Both.
  filename: string;
  content_type?: string;

  // Download
  original_filename?: string;
  mime_type?: string;
};

type FileUploadTable = {
  name: string | undefined;
  single_file: FileUpload | undefined;
  multiple_files: FileUpload[] | undefined;
};

test("file upload base64 tests", async () => {
  const client = await connect();
  const api = client.records<FileUploadTable>("file_upload_table");

  const testBytes1 = new Uint8Array([0, 1, 2, 3, 4, 5]);
  const testBytes2 = new Uint8Array([42, 5, 42, 5]);
  const testBytes3 = new Uint8Array([255, 128, 64, 32]);

  // Test creating a record with multiple base64 encoded files
  const recordId = await api.create({
    name: "Base64 File Upload Test",
    single_file: {
      name: "single_test",
      filename: "test1.bin",
      content_type: "application/octet-stream",
      data: urlSafeBase64Encode(String.fromCharCode(...testBytes1)),
    },
    multiple_files: [
      {
        name: "multi_test_1",
        filename: "test2.bin",
        content_type: "application/octet-stream",
        data: urlSafeBase64Encode(String.fromCharCode(...testBytes2)),
      },
      {
        name: "multi_test_2",
        filename: "test3.bin",
        content_type: "application/octet-stream",
        data: btoa(String.fromCharCode(...testBytes3)), // Standard base64
      },
    ],
  });

  // Read the record back to verify file metadata was stored correctly
  const record = await api.read(recordId);

  expect(record.single_file).not.toBeUndefined();
  expect(record.multiple_files).not.toBeUndefined();

  const singleFile = record.single_file!;
  const multipleFiles = record.multiple_files!;

  // Verify single file metadata
  expect(singleFile.original_filename).toBe("test1.bin");
  expect(singleFile.content_type).toBe("application/octet-stream");

  // Verify multiple files metadata
  expect(multipleFiles.length).toBe(2);
  expect(multipleFiles[0].original_filename).toBe("test2.bin");
  expect(multipleFiles[0].filename.startsWith("test2"));
  expect(multipleFiles[0].filename.endsWith(".bin"));
  expect(multipleFiles[1].original_filename).toBe("test3.bin");

  // Test file download endpoints to verify actual file content
  const singleFileResponse = await fetch(
    `http://${ADDRESS}${filePath("file_upload_table", recordId, "single_file")}`,
  );
  expect(new Uint8Array(await singleFileResponse.arrayBuffer())).toEqual(
    testBytes1,
  );

  const singleFilesResponse = await fetch(
    `http://${ADDRESS}${filesPath("file_upload_table", recordId, "single_file", singleFile.filename)}`,
  );
  expect(new Uint8Array(await singleFilesResponse.arrayBuffer())).toEqual(
    testBytes1,
  );

  const multiFile1Response = await fetch(
    `http://${ADDRESS}${filesPath("file_upload_table", recordId, "multiple_files", multipleFiles[0].filename)}`,
  );
  expect(new Uint8Array(await multiFile1Response.arrayBuffer())).toEqual(
    testBytes2,
  );

  const multiFile2Response = await fetch(
    `http://${ADDRESS}${filesPath("file_upload_table", recordId, "multiple_files", multipleFiles[1].filename)}`,
  );
  expect(new Uint8Array(await multiFile2Response.arrayBuffer())).toEqual(
    testBytes3,
  );

  // Clean up
  await api.delete(recordId);
});
