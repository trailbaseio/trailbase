import * as JSON from "@ungap/raw-json";
import { FeatureCollection } from "geojson";

import { isDev, jsonContentTypeHeader } from "./constants";
import { parseJSON } from "./json";
import { Client } from "./client";

import type { WsProtocol } from "@bindings/WsProtocol";

export interface FileUpload {
  content_type?: null | string;
  filename?: null | string;
  mime_type?: null | string;
  objectstore_path: string;
}

export type CompareOp =
  | "equal"
  | "notEqual"
  | "lessThan"
  | "lessThanEqual"
  | "greaterThan"
  | "greaterThanEqual"
  | "like"
  | "regexp"
  | "@within"
  | "@intersects"
  | "@contains";

function formatCompareOp(op: CompareOp): string {
  switch (op) {
    case "equal":
      return "$eq";
    case "notEqual":
      return "$ne";
    case "lessThan":
      return "$lt";
    case "lessThanEqual":
      return "$lte";
    case "greaterThan":
      return "$gt";
    case "greaterThanEqual":
      return "$gte";
    case "like":
      return "$like";
    case "regexp":
      return "$re";
    // Geospatials:
    case "@within":
    case "@intersects":
    case "@contains":
      return op;
  }
}

export type Filter = {
  column: string;
  op?: CompareOp;
  value: string;
};

export type And = {
  and: FilterOrComposite[];
};

export type Or = {
  or: FilterOrComposite[];
};

export type FilterOrComposite = Filter | And | Or;

export type RecordId = string | number;

export type ChangeEvent =
  | { Insert: object }
  | { Update: object }
  | { Delete: object }
  | { Error: string };

// Re-export type publicly as `Event`. We cannot use `Event` to prevent rollup
// from renaming to `Event_2` to avoid a possible collision with the DOM
// `Event` type (KeyboardEvent, MouseEvent, ...).
export type Event = ChangeEvent;

// TODO: Use `ts-rs` generated types.
interface CreateOp {
  Create: {
    api_name: string;
    value: Record<string, unknown>;
  };
}

interface UpdateOp {
  Update: {
    api_name: string;
    record_id: RecordId;
    value: Record<string, unknown>;
  };
}

interface DeleteOp {
  Delete: {
    api_name: string;
    record_id: RecordId;
  };
}

export interface DeferredOperation<ResponseType> {
  query(): Promise<ResponseType>;
}

// eslint-disable-next-line @typescript-eslint/no-empty-object-type
export interface DeferredMutation<
  ResponseType,
> extends DeferredOperation<ResponseType> {}

export class CreateOperation<
  T = Record<string, unknown>,
> implements DeferredMutation<RecordId> {
  constructor(
    private readonly client: Client,
    private readonly apiName: string,
    private readonly record: Partial<T>,
  ) {}

  async query(): Promise<RecordId> {
    const response = await this.client.fetch(
      `${recordApiBasePath}/${this.apiName}`,
      {
        method: "POST",
        body: JSON.stringify(this.record),
        headers: jsonContentTypeHeader,
      },
    );

    return parseJSON(await response.text()).ids[0];
  }

  protected toJSON(): CreateOp {
    return {
      Create: {
        api_name: this.apiName,
        value: this.record,
      },
    };
  }
}

export class UpdateOperation<
  T = Record<string, unknown>,
> implements DeferredMutation<void> {
  constructor(
    private readonly client: Client,
    private readonly apiName: string,
    private readonly id: RecordId,
    private readonly record: Partial<T>,
  ) {}

  async query(): Promise<void> {
    await this.client.fetch(`${recordApiBasePath}/${this.apiName}/${this.id}`, {
      method: "PATCH",
      body: JSON.stringify(this.record),
      headers: jsonContentTypeHeader,
    });
  }

  protected toJSON(): UpdateOp {
    return {
      Update: {
        api_name: this.apiName,
        record_id: this.id,
        value: this.record,
      },
    };
  }
}

export class DeleteOperation implements DeferredMutation<void> {
  constructor(
    private readonly client: Client,
    private readonly apiName: string,
    private readonly id: RecordId,
  ) {}
  async query(): Promise<void> {
    await this.client.fetch(`${recordApiBasePath}/${this.apiName}/${this.id}`, {
      method: "DELETE",
    });
  }

  protected toJSON(): DeleteOp {
    return {
      Delete: {
        api_name: this.apiName,
        record_id: this.id,
      },
    };
  }
}

export interface ReadOpts {
  expand?: string[];
}

export class ReadOperation<
  T = Record<string, unknown>,
> implements DeferredOperation<T> {
  constructor(
    private readonly client: Client,
    private readonly apiName: string,
    private readonly id: RecordId,
    private readonly opt?: ReadOpts,
  ) {}

  async query(): Promise<T> {
    const expand = this.opt?.expand;
    const response = await this.client.fetch(
      expand
        ? `${recordApiBasePath}/${this.apiName}/${this.id}?expand=${expand.join(",")}`
        : `${recordApiBasePath}/${this.apiName}/${this.id}`,
    );
    return parseJSON(await response.text()) as T;
  }
}

export type Pagination = {
  cursor?: string;
  limit?: number;
  offset?: number;
};

export type ListResponse<T> = {
  cursor?: string;
  records: T[];
  total_count?: number;
};

export interface ListOpts {
  pagination?: Pagination;
  order?: string[];
  filters?: FilterOrComposite[];
  count?: boolean;
  expand?: string[];
}

export class ListOperation<
  T = Record<string, unknown>,
  R = ListResponse<T>,
> implements DeferredOperation<R> {
  constructor(
    private readonly client: Client,
    private readonly apiName: string,
    private readonly opts?: ListOpts,
    private readonly geojson?: string,
  ) {}
  async query(): Promise<R> {
    const params = new URLSearchParams();
    const pagination = this.opts?.pagination;
    if (pagination) {
      const cursor = pagination.cursor;
      if (cursor) params.append("cursor", cursor);

      const limit = pagination.limit;
      if (limit) params.append("limit", limit.toString());

      const offset = pagination.offset;
      if (offset) params.append("offset", offset.toString());
    }
    const order = this.opts?.order;
    if (order) params.append("order", order.join(","));

    if (this.opts?.count) params.append("count", "true");

    const expand = this.opts?.expand;
    if (expand) params.append("expand", expand.join(","));

    const filters = this.opts?.filters;
    if (filters) {
      for (const filter of filters) {
        addFiltersToParams(params, "filter", filter);
      }
    }

    if (this.geojson) params.append("geojson", this.geojson);

    const response = await this.client.fetch(
      `${recordApiBasePath}/${this.apiName}?${params}`,
    );
    return parseJSON(await response.text()) as R;
  }
}

export interface SubscribeOpts {
  filters?: FilterOrComposite[];
}

export interface RecordApi<T = Record<string, unknown>> {
  list(opts?: ListOpts): Promise<ListResponse<T>>;
  listOp(opts?: ListOpts): ListOperation<T>;
  // For queries on TABLE/VIEWs with geometry columns wantin to return GeoJSON.
  listGeoOp(
    geometryColumn: string,
    opts?: ListOpts,
  ): ListOperation<T, FeatureCollection>;

  read(id: RecordId, opt?: ReadOpts): Promise<T>;
  readOp(id: RecordId, opt?: ReadOpts): ReadOperation<T>;

  create(record: T): Promise<RecordId>;
  createOp(record: T): CreateOperation<T>;
  // TODO: Retire in favor of `client.execute`.
  createBulk(records: T[]): Promise<RecordId[]>;

  update(id: RecordId, record: Partial<T>): Promise<void>;
  updateOp(id: RecordId, record: Partial<T>): UpdateOperation;

  delete(id: RecordId): Promise<void>;
  deleteOp(id: RecordId): DeleteOperation;

  subscribe(id: RecordId): Promise<ReadableStream<ChangeEvent>>;
  subscribeAll(opts?: SubscribeOpts): Promise<ReadableStream<ChangeEvent>>;
}

/// Provides CRUD access to records through TrailBase's record API.
export class RecordApiImpl<
  T = Record<string, unknown>,
> implements RecordApi<T> {
  constructor(
    private readonly client: Client,
    private readonly name: string,
  ) {}

  public async list(opts?: ListOpts): Promise<ListResponse<T>> {
    return new ListOperation<T>(this.client, this.name, opts).query();
  }

  public listOp(opts?: ListOpts): ListOperation<T> {
    return new ListOperation<T>(this.client, this.name, opts);
  }

  public listGeoOp(
    geometryColumn: string,
    opts?: ListOpts,
  ): ListOperation<T, FeatureCollection> {
    return new ListOperation<T, FeatureCollection>(
      this.client,
      this.name,
      opts,
      geometryColumn,
    );
  }

  public async read<T = Record<string, unknown>>(
    id: RecordId,
    opt?: ReadOpts,
  ): Promise<T> {
    return new ReadOperation<T>(this.client, this.name, id, opt).query();
  }

  public readOp(id: RecordId, opt?: ReadOpts): ReadOperation<T> {
    return new ReadOperation<T>(this.client, this.name, id, opt);
  }

  public async create(record: T): Promise<RecordId> {
    return new CreateOperation<T>(this.client, this.name, record).query();
  }

  public createOp(record: T): CreateOperation<T> {
    return new CreateOperation<T>(this.client, this.name, record);
  }
  public async createBulk<T = Record<string, unknown>>(
    records: T[],
  ): Promise<RecordId[]> {
    const response = await this.client.fetch(
      `${recordApiBasePath}/${this.name}`,
      {
        method: "POST",
        body: JSON.stringify(records),
        headers: jsonContentTypeHeader,
      },
    );

    return parseJSON(await response.text()).ids;
  }

  public async update(id: RecordId, record: Partial<T>): Promise<void> {
    return new UpdateOperation<T>(this.client, this.name, id, record).query();
  }

  public updateOp(id: RecordId, record: Partial<T>): UpdateOperation<T> {
    return new UpdateOperation<T>(this.client, this.name, id, record);
  }

  public async delete(id: RecordId): Promise<void> {
    return new DeleteOperation(this.client, this.name, id).query();
  }

  public deleteOp(id: RecordId): DeleteOperation {
    return new DeleteOperation(this.client, this.name, id);
  }

  public async subscribe(id: RecordId): Promise<ReadableStream<ChangeEvent>> {
    return await this.subscribeImpl(id);
  }

  public async subscribeAll(
    opts?: SubscribeOpts,
  ): Promise<ReadableStream<ChangeEvent>> {
    return await this.subscribeImpl("*", opts);
  }

  private async subscribeImpl(
    id: RecordId,
    opts?: SubscribeOpts,
  ): Promise<ReadableStream<ChangeEvent>> {
    const params = new URLSearchParams();
    const filters = opts?.filters ?? [];
    if (filters.length > 0) {
      for (const filter of filters) {
        addFiltersToParams(params, "filter", filter);
      }
    }

    const response = await this.client.fetch(
      filters.length > 0
        ? `${recordApiBasePath}/${this.name}/subscribe/${id}?${params}`
        : `${recordApiBasePath}/${this.name}/subscribe/${id}`,
    );
    const body = response.body;
    if (!body) {
      throw Error("Subscription reader is null.");
    }

    const decoder = new TextDecoder();
    const transformStream = new TransformStream<Uint8Array, ChangeEvent>({
      transform(chunk: Uint8Array, controller) {
        const messages = decoder.decode(chunk).trimEnd().split("\n\n");
        for (const msg of messages) {
          if (msg.startsWith("data: ")) {
            controller.enqueue(parseJSON(msg.substring(6)));
          }
        }
      },
      flush(controller) {
        controller.terminate();
      },
    });

    return body.pipeThrough(transformStream);
  }

  async subscribeWs(
    id: RecordId,
    opts?: SubscribeOpts,
  ): Promise<ReadableStream<ChangeEvent>> {
    const params = new URLSearchParams();
    params.append("ws", "true");

    const filters = opts?.filters ?? [];
    if (filters.length > 0) {
      for (const filter of filters) {
        addFiltersToParams(params, "filter", filter);
      }
    }

    return new Promise<ReadableStream<ChangeEvent>>((resolve, reject) => {
      const host = this.client.base?.host ?? "";
      const protocol = this.client.base?.protocol === "https" ? "wss" : "ws";
      const url = `${protocol}://${host}${recordApiBasePath}/${this.name}/subscribe/${id}?${params}`;

      const socket = new WebSocket(url);

      const timeout = setTimeout(() => {
        reject("WS connection timeout");
      }, 5000);

      const readable = new ReadableStream({
        start: (controller) => {
          socket.addEventListener("open", (_openEvent) => {
            // Initialize connection and authenticate.
            socket.send(
              JSON.stringify({
                Init: {
                  auth_token: this.client.tokens()?.auth_token ?? null,
                },
              } as WsProtocol),
            );

            clearTimeout(timeout);
            resolve(readable);
          });

          socket.addEventListener("close", () => {
            controller.close();
          });

          socket.addEventListener("error", (err) => {
            controller.error(err);
          });

          // Listen for messages
          socket.addEventListener("message", (event) => {
            if (typeof event.data !== "string") {
              new Error("expected JSON string");
            }
            controller.enqueue(parseJSON(event.data));
          });
        },
        cancel: () => {
          socket.close();
        },
      });
    });
  }
}

export function filePath(
  apiName: string,
  recordId: RecordId,
  columnName: string,
): string {
  return `${recordApiBasePath}/${apiName}/${recordId}/file/${columnName}`;
}

export function filesPath(
  apiName: string,
  recordId: RecordId,
  columnName: string,
  fileName: string,
): string {
  return `${recordApiBasePath}/${apiName}/${recordId}/files/${columnName}/${fileName}`;
}

function addFiltersToParams(
  params: URLSearchParams,
  path: string,
  filter: FilterOrComposite,
) {
  if ("and" in filter) {
    for (const [i, f] of (filter as And).and.entries()) {
      addFiltersToParams(params, `${path}[$and][${i}]`, f);
    }
  } else if ("or" in filter) {
    for (const [i, f] of (filter as Or).or.entries()) {
      addFiltersToParams(params, `${path}[$or][${i}]`, f);
    }
  } else {
    const f = filter as Filter;
    const op = f.op;
    if (op) {
      params.append(`${path}[${f.column}][${formatCompareOp(op)}]`, f.value);
    } else {
      params.append(`${path}[${f.column}]`, f.value);
    }
  }
}

const recordApiBasePath = "/api/records/v1";

export const exportedForTesting = isDev
  ? {
      subscribeWs: (api: RecordApiImpl, id: RecordId) => api.subscribeWs(id),
    }
  : undefined;
