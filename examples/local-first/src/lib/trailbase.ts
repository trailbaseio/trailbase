import { type RecordApi } from "trailbase";

import type { CollectionConfig, SyncConfig, UtilsRecord } from "@tanstack/db";
import { Store } from "@tanstack/store";

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
   * Record API name
   */
  recordApi: RecordApi<TItem>;
}

export type AwaitTxIdFn = (txId: string, timeout?: number) => Promise<boolean>;

export type RefetchFn = () => Promise<void>;

export interface TrailBaseCollectionUtils extends UtilsRecord {
  refetch: RefetchFn;
}

export function trailBaseCollectionOptions<TItem extends object>(
  config: TrailBaseCollectionConfig<TItem>,
): CollectionConfig<TItem> & { utils: TrailBaseCollectionUtils } {
  const getKey = config.getKey;

  const seenIds = new Store(new Map<string, number>());

  const awaitIds = (
    ids: string[],
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
        const notExpired = curr.entries().filter(([_, v]) => {
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
  let syncParams: SyncParams | undefined;
  const sync = {
    sync: (params: SyncParams) => {
      syncParams = params;
      const { begin, write, commit } = params;

      // Initial fetch.
      async function initialFetch() {
        let response = await config.recordApi.list({ count: true });
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

          response = await config.recordApi.list({
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
        const eventStream = await config.recordApi.subscribe("*");

        for await (const event of eventStream) {
          console.debug(`Event: ${JSON.stringify(event)}`);

          begin();
          let value: TItem | undefined;
          if ("Insert" in event) {
            value = event.Insert as TItem;
            write({ type: "insert", value });
          } else if ("Delete" in event) {
            value = event.Delete as TItem;
            write({ type: "delete", value });
          } else if ("Update" in event) {
            value = event.Update as TItem;
            write({ type: "update", value });
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
      const ids = await config.recordApi.createBulk(
        params.transaction.mutations.map((tx) => {
          const { type, changes } = tx;
          if (type !== "insert") {
            throw new Error(`Expected 'insert', got: ${type}`);
          }
          return changes as TItem;
        }),
      );

      // The optimistic mutation overlay is removed on return, so at this point
      // we have to ensure that the new record was properly added to the local
      // DB by the subscription.
      await awaitIds(ids.map((id) => String(id)));

      return ids;
    },
    onUpdate: async (params) => {
      const ids: string[] = await Promise.all(
        params.transaction.mutations.map(async (tx) => {
          const { type, changes, key } = tx;
          if (type !== "update") {
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
      const ids: string[] = await Promise.all(
        params.transaction.mutations.map(async (tx) => {
          const { type, key } = tx;
          if (type !== "delete") {
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
      // NOTE: Refetch shouldn't be necessary, we'll see. It may still be
      // necessary if subscriptions gets temporarily disconnected and changes
      // get lost.
      refetch: async () => {
        console.warn(`Not implemented: refetch`, syncParams?.collection);
      },
    },
  };
}
