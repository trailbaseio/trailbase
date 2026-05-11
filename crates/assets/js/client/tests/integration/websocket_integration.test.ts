import { expect, test } from "vitest";

import type {
  ChangeDeleteEvent,
  ChangeInsertEvent,
  ChangeUpdateEvent,
  Event,
  RecordApiImpl,
} from "../../src/index";
import { exportedForTesting as recordExportForTesting } from "../../src/record_api";

import { connect } from "../setup";
import { SimpleStrict, NewSimpleStrict } from "../simple_strict";

const { subscribeWs } = recordExportForTesting!;

test("Subscribe to entire table via WebSocket", async () => {
  const client = await connect();
  const api = client.records<NewSimpleStrict>("simple_strict_table");

  const eventStream = await subscribeWs(api as RecordApiImpl, "*");

  const now = new Date().getTime();
  const createMessage = `ts client ws realtime test 0: =?&${now}`;
  const id = (await api.create({
    text_not_null: createMessage,
  })) as string;

  const updatedMessage = `ts client ws updated realtime test 0: ${now}`;
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
  expect(
    ((events[0] as ChangeInsertEvent)["Insert"] as SimpleStrict)[
      "text_not_null"
    ],
  ).equals(createMessage);
  expect(
    ((events[1] as ChangeUpdateEvent)["Update"] as SimpleStrict)[
      "text_not_null"
    ],
  ).equals(updatedMessage);
  expect(
    ((events[2] as ChangeDeleteEvent)["Delete"] as SimpleStrict)[
      "text_not_null"
    ],
  ).equals(updatedMessage);
});
