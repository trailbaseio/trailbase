import { IncomingRequest, ResponseOutparam } from "wasi:http/types@0.2.12";
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
      getManifest: function (jsonArgs: string): string {
        const args: InitArguments = JSON.parse(jsonArgs);

        opts.init?.({
          version: args.version ?? undefined,
        });

        const subsystems = args.subsystems;

        const http_handlers = subsystems?.find((v) => v === "http")
          ? (opts.httpHandlers?.map((h) => ({
              method: h.method,
              path: h.path,
            })) ?? null)
          : null;

        const job_handlers = subsystems?.find((v) => v === "jobs")
          ? (opts.jobHandlers?.map((h) => ({ name: h.name, spec: h.spec })) ??
            null)
          : null;

        const manifest: InitManifest = {
          metadata: null,
          http_handlers,
          job_handlers,
          sqlite_functions: null,
        };

        return JSON.stringify(manifest);
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
