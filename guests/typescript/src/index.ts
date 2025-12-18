import { IncomingRequest, ResponseOutparam } from "wasi:http/types@0.2.3";
import type {
  Arguments,
  HttpHandlers,
  JobHandlers,
  SqliteFunctions,
} from "trailbase:component/init-endpoint@0.1.0";
import type {
  Arguments as SqliteArguments,
  Error as SqliteError,
  dispatchScalarFunction,
} from "trailbase:component/sqlite-function-endpoint@0.1.0";
import type { HttpHandlerInterface } from "./http";
import type { JobHandlerInterface } from "./job";
import { buildIncomingHttpHandler } from "./http/incoming";

export { addPeriodicCallback } from "./timer";

export * from "./util";

export interface Config {
  incomingHandler: {
    handle: (
      req: IncomingRequest,
      respOutparam: ResponseOutparam,
    ) => Promise<void>;
  };
  initEndpoint: {
    initHttpHandlers: (args: Arguments) => HttpHandlers;
    initJobHandlers: (args: Arguments) => JobHandlers;
    initSqliteFunctions: (args: Arguments) => SqliteFunctions;
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
      initHttpHandlers: function (args: Arguments): HttpHandlers {
        opts.init?.({
          version: args.version,
        });

        return {
          handlers: (opts.httpHandlers ?? []).map((h) => [h.method, h.path]),
        };
      },
      initJobHandlers: function (args: Arguments): JobHandlers {
        opts.init?.({
          version: args.version,
        });

        return {
          handlers: (opts.jobHandlers ?? []).map((h) => [h.name, h.spec]),
        };
      },
      initSqliteFunctions: function (args: Arguments): SqliteFunctions {
        opts.init?.({
          version: args.version,
        });

        return {
          scalarFunctions: [],
        };
      },
    },
    sqliteFunctionEndpoint: {
      dispatchScalarFunction: function (_args: SqliteArguments) {
        throw {
          tag: "other",
          val: "missing sqlite function",
        } as SqliteError;
      },
    },
  };
}
