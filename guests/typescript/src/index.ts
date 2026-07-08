import { IncomingRequest, ResponseOutparam } from "wasi:http/types@0.2.12";
import type {
  // Arguments,
  // HttpHandlers,
  // JobHandlers,
  // SqliteFunctions,
} from "trailbase:component/init-endpoint@0.2.0";
import type {
  Arguments as SqliteArguments,
  Error as SqliteError,
  dispatchScalarFunction,
} from "trailbase:component/sqlite-function-endpoint@0.2.0";
import type { HttpHandlerInterface } from "./http";
import type { JobHandlerInterface } from "./job";
import { buildIncomingHttpHandler } from "./http/incoming";

export { addPeriodicCallback } from "./timer";

import type { InitArguments } from "@common/InitArguments";
import type { InitManifest } from "@common/InitManifest";

export * from "./util";

export interface Config {
  incomingHandler: {
    handle: (
      req: IncomingRequest,
      respOutparam: ResponseOutparam,
    ) => Promise<void>;
  };
  initEndpoint: {
    // initHttpHandlers: (args: Arguments) => HttpHandlers;
    // initJobHandlers: (args: Arguments) => JobHandlers;
    // initSqliteFunctions: (args: Arguments) => SqliteFunctions;
    getManifest: (args: string) => string;
  };
  sqliteFunctionEndpoint: {
    dispatchScalarFunction: typeof dispatchScalarFunction;
  };
}

export interface InitArgs {
  version: string | undefined;
}

export function defineConfig(opts: {
  init?: (args: InitArgs) => void;
  httpHandlers?: HttpHandlerInterface[];
  jobHandlers?: JobHandlerInterface[];
}): Config {
  return {
    incomingHandler: {
      handle: buildIncomingHttpHandler(opts),
    },
    initEndpoint: {
      // initHttpHandlers: function(args: Arguments): HttpHandlers {
      //   opts.init?.({
      //     version: args.version,
      //   });
      //
      //   return {
      //     handlers: (opts.httpHandlers ?? []).map((h) => [h.method, h.path]),
      //   };
      // },
      // initJobHandlers: function(args: Arguments): JobHandlers {
      //   opts.init?.({
      //     version: args.version,
      //   });
      //
      //   return {
      //     handlers: (opts.jobHandlers ?? []).map((h) => [h.name, h.spec]),
      //   };
      // },
      // initSqliteFunctions: function(args: Arguments): SqliteFunctions {
      //   opts.init?.({
      //     version: args.version,
      //   });
      //
      //   return {
      //     scalarFunctions: [],
      //   };
      // },
      getManifest: function(jsonArgs: string): string {
        const args: InitArguments = JSON.parse(jsonArgs);

        opts.init?.({
          version: args.version ?? undefined,
        });

        const manifest: InitManifest = {
          http_handlers: opts.httpHandlers?.map((h) => ({ method: h.method, path: h.path })) ?? null,
          job_handlers: opts.jobHandlers?.map((h) => ({ name: h.name, spec: h.spec })) ?? null,
          sqlite_functions: null,
        };

        return JSON.stringify(manifest);
      },
    },
    sqliteFunctionEndpoint: {
      dispatchScalarFunction: function(_args: SqliteArguments) {
        throw {
          tag: "other",
          val: "missing sqlite function",
        } as SqliteError;
      },
    },
  };
}
