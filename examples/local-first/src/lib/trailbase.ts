import { Client } from "trailbase";

import { Store } from "@tanstack/store";
import type {
  CollectionConfig,
  SyncConfig,
  MutationFnParams,
  UtilsRecord,
} from "@tanstack/db";

/**
 * Configuration interface for TrailbaseCollection
 */
export interface TrailBaseCollectionConfig<TItem extends object>
  extends Omit<CollectionConfig<TItem>, `sync`> {
  /**
   * TrailBase client.
   */
  client: Client;

  /**
   * Record API name
   */
  recordApi: string;

  // getKey: CollectionConfig<TItem>[`getKey`];
}

export type AwaitTxIdFn = (txId: string, timeout?: number) => Promise<boolean>;

export type RefetchFn = () => Promise<void>;

export interface TrailBaseCollectionUtils extends UtilsRecord {
  refetch: RefetchFn;
}

export function trailBaseCollectionOptions<TItem extends object>(
  config: TrailBaseCollectionConfig<TItem>,
): CollectionConfig<TItem> & { utils: TrailBaseCollectionUtils } {
  const seenTxids = new Store<Set<string>>(new Set([Math.random().toString()]));
  const sync = createTrailBaseSync<TItem>(config, seenTxids);

  /**
   * Wait for a specific transaction ID to be synced
   * @param txId The transaction ID to wait for as a string
   * @param timeout Optional timeout in milliseconds (defaults to 30000ms)
   * @returns Promise that resolves when the txId is synced
   */
  const awaitTxId: AwaitTxIdFn = async (
    txId: string,
    timeout = 30000,
  ): Promise<boolean> => {
    const hasTxid = seenTxids.state.has(txId);
    if (hasTxid) return true;

    return new Promise((resolve, reject) => {
      const timeoutId = setTimeout(() => {
        unsubscribe();
        reject(new Error(`Timeout waiting for txId: ${txId}`));
      }, timeout);

      const unsubscribe = seenTxids.subscribe(() => {
        if (seenTxids.state.has(txId)) {
          clearTimeout(timeoutId);
          unsubscribe();
          resolve(true);
        }
      });
    });
  };

  // Create wrapper handlers for direct persistence operations that handle txid awaiting
  const onInsert = async (params: MutationFnParams<TItem>) => {
    const handlerResult = (await config.onInsert!(params)) ?? {};
    const txid = (handlerResult as { txid?: string }).txid;

    if (!txid) {
      throw new Error(
        `Electric collection onInsert handler must return a txid`,
      );
    }

    await awaitTxId(txid);
    return handlerResult;
  };

  const onUpdate = async (params: MutationFnParams<TItem>) => {
    const handlerResult = await config.onUpdate!(params);
    const txid = (handlerResult as { txid?: string }).txid;

    if (!txid) {
      throw new Error(
        `Electric collection onUpdate handler must return a txid`,
      );
    }

    await awaitTxId(txid);
    return handlerResult;
  };

  const onDelete = async (params: MutationFnParams<TItem>) => {
    const handlerResult = await config.onDelete!(params);
    const txid = (handlerResult as { txid?: string }).txid;

    if (!txid) {
      throw new Error(
        `Electric collection onDelete handler must return a txid`,
      );
    }

    await awaitTxId(txid);
    return handlerResult;
  };

  return {
    sync,
    getKey: config.getKey,
    onInsert,
    onUpdate,
    onDelete,
    utils: {
      refetch: async () => {},
    },
  };
}

/**
 * Internal function to create TrailBase sync configuration.
 */
function createTrailBaseSync<TItem extends object>(
  config: TrailBaseCollectionConfig<TItem>,
  seenTxIds: Store<Set<string>>,
): SyncConfig<TItem> {
  const client = config.client;

  return {
    sync: (params: Parameters<SyncConfig<TItem>[`sync`]>[0]) => {
      const { begin, write, commit } = params;

      const records = client.records(config.recordApi);
      (async () => {
        const eventStream = await records.subscribe("*");

        for await (const event of eventStream) {
          console.log(`${event}`);

          seenTxIds.setState((currentTxids) => {
            const clonedSeen = new Set(currentTxids);
            // newTxids.forEach((txid) => clonedSeen.add(txid))
            //
            // newTxids = new Set()
            return clonedSeen;
          });
        }
      })();
    },
    // Expose the getSyncMetadata function
    getSyncMetadata: undefined,
  };
}
