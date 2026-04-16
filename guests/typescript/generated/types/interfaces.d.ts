/// <reference path="./interfaces/trailbase-component-init-endpoint.d.ts" />
/// <reference path="./interfaces/trailbase-component-sqlite-function-endpoint.d.ts" />
/// <reference path="./interfaces/trailbase-database-sqlite.d.ts" />
/// <reference path="./interfaces/wasi-clocks-monotonic-clock.d.ts" />
/// <reference path="./interfaces/wasi-clocks-wall-clock.d.ts" />
/// <reference path="./interfaces/wasi-filesystem-preopens.d.ts" />
/// <reference path="./interfaces/wasi-filesystem-types.d.ts" />
/// <reference path="./interfaces/wasi-http-incoming-handler.d.ts" />
/// <reference path="./interfaces/wasi-http-outgoing-handler.d.ts" />
/// <reference path="./interfaces/wasi-http-types.d.ts" />
/// <reference path="./interfaces/wasi-io-error.d.ts" />
/// <reference path="./interfaces/wasi-io-poll.d.ts" />
/// <reference path="./interfaces/wasi-io-streams.d.ts" />
/// <reference path="./interfaces/wasi-keyvalue-atomics.d.ts" />
/// <reference path="./interfaces/wasi-keyvalue-batch.d.ts" />
/// <reference path="./interfaces/wasi-keyvalue-store.d.ts" />
/// <reference path="./interfaces/wasi-random-insecure-seed.d.ts" />
/// <reference path="./interfaces/wasi-random-insecure.d.ts" />
/// <reference path="./interfaces/wasi-random-random.d.ts" />
declare module "trailbase:component/interfaces@0.2.0" {
  export type * as TrailbaseDatabaseSqlite020 from "trailbase:database/sqlite@0.2.0"; // import trailbase:database/sqlite@0.2.0
  export type * as WasiClocksMonotonicClock023 from "wasi:clocks/monotonic-clock@0.2.3"; // import wasi:clocks/monotonic-clock@0.2.3
  export type * as WasiClocksWallClock023 from "wasi:clocks/wall-clock@0.2.3"; // import wasi:clocks/wall-clock@0.2.3
  export type * as WasiFilesystemPreopens023 from "wasi:filesystem/preopens@0.2.3"; // import wasi:filesystem/preopens@0.2.3
  export type * as WasiFilesystemTypes023 from "wasi:filesystem/types@0.2.3"; // import wasi:filesystem/types@0.2.3
  export type * as WasiHttpOutgoingHandler023 from "wasi:http/outgoing-handler@0.2.3"; // import wasi:http/outgoing-handler@0.2.3
  export type * as WasiHttpTypes023 from "wasi:http/types@0.2.3"; // import wasi:http/types@0.2.3
  export type * as WasiIoError023 from "wasi:io/error@0.2.3"; // import wasi:io/error@0.2.3
  export type * as WasiIoPoll023 from "wasi:io/poll@0.2.3"; // import wasi:io/poll@0.2.3
  export type * as WasiIoStreams023 from "wasi:io/streams@0.2.3"; // import wasi:io/streams@0.2.3
  export type * as WasiKeyvalueAtomics020Draft from "wasi:keyvalue/atomics@0.2.0-draft"; // import wasi:keyvalue/atomics@0.2.0-draft
  export type * as WasiKeyvalueBatch020Draft from "wasi:keyvalue/batch@0.2.0-draft"; // import wasi:keyvalue/batch@0.2.0-draft
  export type * as WasiKeyvalueStore020Draft from "wasi:keyvalue/store@0.2.0-draft"; // import wasi:keyvalue/store@0.2.0-draft
  export type * as WasiRandomInsecureSeed023 from "wasi:random/insecure-seed@0.2.3"; // import wasi:random/insecure-seed@0.2.3
  export type * as WasiRandomInsecure023 from "wasi:random/insecure@0.2.3"; // import wasi:random/insecure@0.2.3
  export type * as WasiRandomRandom023 from "wasi:random/random@0.2.3"; // import wasi:random/random@0.2.3
  export * as incomingHandler from "wasi:http/incoming-handler@0.2.3"; // export wasi:http/incoming-handler@0.2.3
  export * as initEndpoint from "trailbase:component/init-endpoint@0.2.0"; // export trailbase:component/init-endpoint@0.2.0
  export * as sqliteFunctionEndpoint from "trailbase:component/sqlite-function-endpoint@0.2.0"; // export trailbase:component/sqlite-function-endpoint@0.2.0
}
