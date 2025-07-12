import { Store } from "@tanstack/store";
import type { Event, RecordApi } from "trailbase";

import type { CollectionConfig, SyncConfig, UtilsRecord } from "@tanstack/db";

/**
 * Configuration interface for TrailbaseCollection
 */
export interface TrailBaseCollectionConfig<
  TItem extends object,
  TKey extends string | number = string | number,
> extends Omit<
    CollectionConfig<TItem, TKey>,
    `sync` | `onInsert` | `onUpdate` | `onDelete`
  > {
  /**
   * Record API name
   */
  recordApi: RecordApi<TItem>;
}

export type AwaitTxIdFn = (txId: string, timeout?: number) => Promise<boolean>;

export interface TrailBaseCollectionUtils extends UtilsRecord {
  cancel: () => void;
}

export function trailBaseCollectionOptions<TItem extends object>(
  config: TrailBaseCollectionConfig<TItem>,
): CollectionConfig<TItem> & { utils: TrailBaseCollectionUtils } {
  const getKey = config.getKey;

  const seenIds = new Store(new Map<string, number>());

  const awaitIds = (
    ids: Array<string>,
    timeout: number = 120 * 1000,
  ): Promise<void> => {
    const completed = (value: Map<string, number>) =>
      ids.every((id) => value.has(id));
    if (completed(seenIds.state)) {
      return Promise.resolve();
    }

    return new Promise<void>((resolve, reject) => {
      const timeoutId = setTimeout(() => {
        unsubscribe();
        reject(new Error(`Timeout waiting for ids: ${ids}`));
      }, timeout);

      const unsubscribe = seenIds.subscribe((value) => {
        if (completed(value.currentVal)) {
          clearTimeout(timeoutId);
          unsubscribe();
          resolve();
        }
      });
    });
  };

  const weakSeenIds = new WeakRef(seenIds);
  const cleanupTimer = setInterval(() => {
    const seen = weakSeenIds.deref();
    if (seen) {
      seen.setState((curr) => {
        const now = Date.now();
        let anyExpired = false;

        const notExpired = Array.from(curr.entries()).filter(([_, v]) => {
          const expired = now - v > 300 * 1000;
          anyExpired = anyExpired || expired;
          return !expired;
        });

        if (anyExpired) {
          return new Map(notExpired);
        }
        return curr;
      });
    } else {
      clearInterval(cleanupTimer);
    }
  }, 120 * 1000);

  type SyncParams = Parameters<SyncConfig<TItem>[`sync`]>[0];

  let eventReader: ReadableStreamDefaultReader<Event> | undefined;
  const cancel = () => {
    if (eventReader) {
      eventReader.cancel();
      eventReader.releaseLock();
      eventReader = undefined;
    }
  };

  const sync = {
    sync: (params: SyncParams) => {
      const { begin, write, commit } = params;

      // Initial fetch.
      async function initialFetch() {
        const limit = 256;
        let response = await config.recordApi.list({
          pagination: {
            limit,
          },
        });
        let cursor = response.cursor;
        let got = 0;

        begin();

        while (true) {
          const length = response.records.length;
          if (length === 0) break;

          got = got + length;
          for (const item of response.records) {
            write({ type: `insert`, value: item });
          }

          if (length < limit) break;

          response = await config.recordApi.list({
            pagination: {
              limit,
              cursor,
              offset: cursor === undefined ? got : undefined,
            },
          });
          cursor = response.cursor;
        }

        commit();
      }

      // Afterwards subscribe.
      async function listen(reader: ReadableStreamDefaultReader<Event>) {
        while (true) {
          const { done, value: event } = await reader.read();

          if (done || !event) {
            reader.releaseLock();
            eventReader = undefined;
            return;
          }

          begin();
          let value: TItem | undefined;
          if (`Insert` in event) {
            value = event.Insert as TItem;
            write({ type: `insert`, value });
          } else if (`Delete` in event) {
            value = event.Delete as TItem;
            write({ type: `delete`, value });
          } else if (`Update` in event) {
            value = event.Update as TItem;
            write({ type: `update`, value });
          } else {
            console.error(`Error: ${event.Error}`);
          }
          commit();

          if (value) {
            seenIds.setState((curr) => {
              const newIds = new Map(curr);
              newIds.set(String(getKey(value)), Date.now());
              return newIds;
            });
          }
        }
      }

      async function start() {
        const eventStream = await config.recordApi.subscribe(`*`);
        const reader = (eventReader = eventStream.getReader());

        // Start listening for subscriptions first. Otherwise, we'd risk a gap
        // between the initial fetch and starting to listen.
        listen(reader);

        try {
          await initialFetch();
        } catch (e) {
          cancel();
          throw e;
        }
      }

      start();
    },
    // Expose the getSyncMetadata function
    getSyncMetadata: undefined,
  };

  return {
    ...config,
    sync,
    getKey,
    onInsert: async (params): Promise<Array<number | string>> => {
      const ids = await config.recordApi.createBulk(
        params.transaction.mutations.map((tx) => {
          const { type, changes } = tx;
          if (type !== `insert`) {
            throw new Error(`Expected 'insert', got: ${type}`);
          }
          return changes;
        }),
      );

      // The optimistic mutation overlay is removed on return, so at this point
      // we have to ensure that the new record was properly added to the local
      // DB by the subscription.
      await awaitIds(ids.map((id) => String(id)));

      return ids;
    },
    onUpdate: async (params) => {
      const ids: Array<string> = await Promise.all(
        params.transaction.mutations.map(async (tx) => {
          const { type, changes, key } = tx;
          if (type !== `update`) {
            throw new Error(`Expected 'update', got: ${type}`);
          }

          await config.recordApi.update(key, changes);
          return String(key);
        }),
      );

      // The optimistic mutation overlay is removed on return, so at this point
      // we have to ensure that the new record was properly updated in the local
      // DB by the subscription.
      await awaitIds(ids);
    },
    onDelete: async (params) => {
      const ids: Array<string> = await Promise.all(
        params.transaction.mutations.map(async (tx) => {
          const { type, key } = tx;
          if (type !== `delete`) {
            throw new Error(`Expected 'delete', got: ${type}`);
          }

          await config.recordApi.delete(key);
          return String(key);
        }),
      );

      // The optimistic mutation overlay is removed on return, so at this point
      // we have to ensure that the new record was properly updated in the local
      // DB by the subscription.
      await awaitIds(ids);
    },
    utils: {
      cancel,
    },
  };
}
