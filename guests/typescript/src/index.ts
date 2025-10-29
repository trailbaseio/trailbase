import { IncomingRequest, ResponseOutparam } from "wasi:http/types@0.2.3";
import type {
  Arguments,
  HttpHandlers,
  JobHandlers,
} from "trailbase:component/init@0.1.0";
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
  init: {
    initHttpHandlers: (args: Arguments) => HttpHandlers;
    initJobHandlers: (args: Arguments) => JobHandlers;
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
    init: {
      initHttpHandlers: function(args: Arguments): HttpHandlers {
        opts.init?.({
          version: args.version,
        });

        return {
          handlers: (opts.httpHandlers ?? []).map((h) => [
            h.method,
            h.path,
          ]),
        };
      },
      initJobHandlers: function(args: Arguments): JobHandlers {
        opts.init?.({
          version: args.version,
        });

        return {
          handlers: (opts.jobHandlers ?? []).map((h) => [h.name, h.spec]),
        };
      },
    },
  };
}
