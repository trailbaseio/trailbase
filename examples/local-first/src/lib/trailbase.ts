import { Client } from "trailbase";

import { Store } from "@tanstack/store";
import { Collection } from "@tanstack/db";
import type { CollectionConfig, SyncConfig } from "@tanstack/db";

/**
 * Configuration interface for TrailbaseCollection
 */
export interface TrailBaseCollectionConfig<TItem extends object>
  extends Omit<CollectionConfig<TItem>, `sync`> {
  /**
   * TrailBase client.
   */
  client: Client;
}

/**
 * Specialized Collection class for TrailBase integration
 */
export class TrailbaseCollection<
  TItem extends object,
> extends Collection<TItem> {
  private seenTxids: Store<Set<number>>;

  constructor(config: TrailBaseCollectionConfig<TItem>) {
    const seenTxids = new Store<Set<number>>(new Set([Math.random()]));
    super({ ...config, sync: createTrailBaseSync<TItem>(config, seenTxids) });

    this.seenTxids = seenTxids;
  }

  /**
   * Wait for a specific transaction ID to be synced
   * @param txId The transaction ID to wait for
   * @param timeout Optional timeout in milliseconds (defaults to 30000ms)
   * @returns Promise that resolves when the txId is synced
   */
  async awaitTxId(txId: number, timeout = 30000): Promise<boolean> {
    const hasTxid = this.seenTxids.state.has(txId);
    if (hasTxid) return true;

    return new Promise((resolve, reject) => {
      const timeoutId = setTimeout(() => {
        unsubscribe();
        reject(new Error(`Timeout waiting for txId: ${txId}`));
      }, timeout);

      const unsubscribe = this.seenTxids.subscribe(() => {
        if (this.seenTxids.state.has(txId)) {
          clearTimeout(timeoutId);
          unsubscribe();
          resolve(true);
        }
      });
    });
  }
}

export function createTrailBaseCollection<TItem extends object>(
  config: TrailBaseCollectionConfig<TItem>,
): TrailbaseCollection<TItem> {
  return new TrailbaseCollection(config);
}

/**
 * Internal function to create TrailBase sync configuration.
 */
function createTrailBaseSync<TItem extends object>(
  config: TrailBaseCollectionConfig<TItem>,
  seenTxIds: Store<Set<number>>,
): SyncConfig<TItem> {
  const client = config.client;
  return {
    sync: (params: Parameters<SyncConfig<TItem>[`sync`]>[0]) => {
      const { begin, write, commit } = params;

      const records = client.records("foo");
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
