import { IncomingRequest, ResponseOutparam } from "wasi:http/types@0.2.3";
import type {
  InitResult,
  InitArguments,
} from "trailbase:runtime/init-endpoint";
import type { HttpHandlerInterface } from "./http";
import type { JobHandlerInterface } from "./job";
import { buildIncomingHttpHandler } from "./http/incoming";

export { addPeriodicCallback } from "./timer";

export * from "./util";
export type { InitResult } from "trailbase:runtime/init-endpoint";

export interface Config {
  incomingHandler: {
    handle: (
      req: IncomingRequest,
      respOutparam: ResponseOutparam,
    ) => Promise<void>;
  };
  initEndpoint: {
    init: (args: InitArguments) => InitResult;
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
      init: function (args: InitArguments): InitResult {
        opts.init?.({
          version: args.version,
        });

        return {
          httpHandlers: (opts.httpHandlers ?? []).map((h) => [
            h.method,
            h.path,
          ]),
          jobHandlers: (opts.jobHandlers ?? []).map((h) => [h.name, h.spec]),
        };
      },
    },
  };
}
