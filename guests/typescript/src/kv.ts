import { Bucket, open as witOpen } from "wasi:keyvalue/store@0.2.0-draft";

export function open(): Store {
  return new Store(witOpen(""));
}

export class Store {
  constructor(private readonly bucket: Bucket) {}

  static open(): Store {
    return new Store(witOpen(""));
  }

  get(key: string): Uint8Array | undefined {
    return this.bucket.get(key);
  }

  set(key: string, value: Uint8Array) {
    this.bucket.set(key, value);
  }

  delete(key: string) {
    this.bucket.delete(key);
  }

  exists(key: string): boolean {
    return this.bucket.exists(key);
  }
}
