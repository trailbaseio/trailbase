import { IncomingRequest, ResponseOutparam } from "wasi:http/types@0.2.3";
import type { InitResult } from "trailbase:runtime/init-endpoint";
import type { HttpHandlerInterface } from "./http";
import { buildIncomingHttpHandler } from "./http/incoming";

import { addPeriodicCallback } from "./timer";
export const timer = {
  addPeriodicCallback,
};

export * from "./util";
export type { InitResult } from "trailbase:runtime/init-endpoint";
export { threadId } from "trailbase:runtime/host-endpoint";

export type JobHandler = {
  name: string;
  spec: string;
  handler: () => void | Promise<void>;
};

export interface Config {
  incomingHandler: {
    handle: (
      req: IncomingRequest,
      respOutparam: ResponseOutparam,
    ) => Promise<void>;
  };
  initEndpoint: {
    init: () => InitResult;
  };
}

export function defineConfig(args: {
  httpHandlers?: HttpHandlerInterface[];
  jobHandlers?: JobHandler[];
}): Config {
  return {
    incomingHandler: {
      handle: buildIncomingHttpHandler(args),
    },
    initEndpoint: {
      init: function (): InitResult {
        return {
          httpHandlers: (args.httpHandlers ?? []).map((h) => [
            h.method,
            h.path,
          ]),
          jobHandlers: (args.jobHandlers ?? []).map((h) => [h.name, h.spec]),
        };
      },
    },
  };
}
