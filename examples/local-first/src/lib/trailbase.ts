import { Client } from "trailbase";

import type {
  Collection,
  CollectionConfig,
  SyncConfig,
  UtilsRecord,
} from "@tanstack/db";

/**
 * Configuration interface for TrailbaseCollection
 */
export interface TrailBaseCollectionConfig<
  TItem extends object,
  TKey extends string | number = string | number,
> extends Omit<
    CollectionConfig<TItem, TKey>,
    "sync" | "onInsert" | "onUpdate" | "onDelete"
  > {
  /**
   * TrailBase client.
   */
  client: Client;

  /**
   * Record API name
   */
  recordApi: string;
}

export type AwaitTxIdFn = (txId: string, timeout?: number) => Promise<boolean>;

export type RefetchFn = () => Promise<void>;

export interface TrailBaseCollectionUtils extends UtilsRecord {
  refetch: RefetchFn;
}

export function trailBaseCollectionOptions<TItem extends object>(
  config: TrailBaseCollectionConfig<TItem>,
): CollectionConfig<TItem> & { utils: TrailBaseCollectionUtils } {
  const client = config.client;
  const records = client.records(config.recordApi);
  const getKey = config.getKey;

  let coll: Collection<TItem> | undefined;

  const sync = {
    sync: (params: Parameters<SyncConfig<TItem>[`sync`]>[0]) => {
      const { collection, begin, write, commit } = params;
      coll = collection;

      console.debug("sync", params);

      // Initial fetch.
      async function initialFetch() {
        let response = await records.list<TItem>({ count: true });
        let cursor = response.cursor;
        let got = 0;

        begin();

        while (true) {
          const length = response.records.length;
          if (length === 0) {
            break;
          }

          got = got + length;
          for (const item of response.records) {
            write({ type: "insert", value: item as TItem });
          }

          response = await records.list<TItem>({
            pagination: {
              cursor,
              offset: cursor === undefined ? got : undefined,
            },
          });
          cursor = response.cursor;
        }

        commit();
      }

      // Afterwards subscribe.
      async function subscribe() {
        const eventStream = await records.subscribe("*");

        for await (const event of eventStream) {
          console.debug(`Event: ${JSON.stringify(event)}`);

          begin();
          if ("Insert" in event) {
            const value = event.Insert as TItem;
            // const _key = getKey(value);
            write({ type: "insert", value });
          } else if ("Delete" in event) {
            const value = event.Delete as TItem;
            // const _key = getKey(value);
            write({ type: "delete", value });
          } else if ("Update" in event) {
            const value = event.Update as TItem;
            // const _key = getKey(value);
            write({ type: "update", value });
          } else {
            console.error(`Error: ${event.Error}`);
          }
          commit();
        }
      }

      initialFetch().then(() => {
        subscribe();
      });
    },
    // Expose the getSyncMetadata function
    getSyncMetadata: undefined,
  };

  return {
    sync,
    getKey,
    onInsert: async (params): Promise<(number | string)[]> => {
      console.debug("onInsert");

      const inserts = params.transaction.mutations.map((tx) => {
        const { type, changes } = tx;
        console.assert(type === "insert");
        return changes as TItem;
      });

      const ids = await records.createBulk(inserts);
      return ids;
    },
    onUpdate: async (params) => {
      console.error("Not implemented: onUpdate", params);
    },
    onDelete: async (params) => {
      console.error("Not implemented: onDelete", params);
    },
    utils: {
      // NOTE: Refetch shouldn't be necessary, we'll see. It may still be
      // necessary if subscriptions gets temorarily disconnected and changes
      // get lost.
      refetch: async () => {
        console.warn(`Not implemented: refetch`, coll);
      },
    },
  };
}
