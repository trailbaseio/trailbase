import {
  IncomingRequest,
  OutgoingResponse,
  ResponseOutparam,
} from "wasi:http/types@0.2.3";
import type { InitResult } from "trailbase:runtime/init-endpoint";
import type { HttpHandlerInterface, ResponseType } from "./http";
import {
  HttpError,
  RequestImpl,
  StatusCode,
  encodeBytes,
  buildResponse,
} from "./http";
import type { HttpContext } from "@common/HttpContext";

import { addPeriodicCallback, awaitPendingTimers } from "./timer";
export const timer = {
  addPeriodicCallback,
};

// export { Request, OutgoingResponse, StatusCode, ResponseType, HttpError, HttpHandler, HttpHandlerCallback, HttpHandlerInterface, ResponseOptions, buildJsonResponse, buildResponse } from "./http";
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
  const init: InitResult = {
    httpHandlers: (args.httpHandlers ?? []).map((h) => [h.method, h.path]),
    jobHandlers: (args.jobHandlers ?? []).map((h) => [h.name, h.spec]),
  };

  const httpHandlers = Object.fromEntries(
    (args.httpHandlers ?? []).map((h) => [h.path, h.handler]),
  );
  const jobHandlers = Object.fromEntries(
    (args.jobHandlers ?? []).map((h) => [h.name, h.handler]),
  );

  async function handle(req: IncomingRequest): Promise<ResponseType> {
    const path: string | undefined = req.pathWithQuery();
    if (!path) {
      throw new HttpError(StatusCode.NOT_FOUND, "path not found");
    }

    const context: HttpContext = JSON.parse(
      new TextDecoder().decode(req.headers().get("__context")[0]),
    );

    if (context.kind === "Job") {
      const handler = jobHandlers[context.registered_path];
      await handler();
      return new Uint8Array();
    } else {
      const handler = httpHandlers[context.registered_path];
      if (!handler) {
        throw new HttpError(StatusCode.NOT_FOUND, "impl not found");
      }

      return await handler(
        new RequestImpl(
          req.method(),
          req.pathWithQuery() ?? "",
          context.path_params,
          req.scheme(),
          req.authority() ?? "",
          req.headers(),
          context.user,
          req.consume(),
        ),
      );
    }
  }

  return {
    incomingHandler: {
      handle: async function (
        req: IncomingRequest,
        respOutparam: ResponseOutparam,
      ) {
        try {
          const resp: ResponseType = await handle(req);
          writeResponse(
            respOutparam,
            resp instanceof OutgoingResponse
              ? resp
              : buildResponse(
                  resp instanceof Uint8Array ? resp : encodeBytes(resp),
                ),
          );
        } catch (err) {
          if (err instanceof HttpError) {
            writeResponse(
              respOutparam,
              buildResponse(encodeBytes(`${err.message}\n`), {
                status: err.statusCode,
              }),
            );
          } else {
            writeResponse(
              respOutparam,
              buildResponse(encodeBytes(`Other: ${err}\n`), {
                status: StatusCode.INTERNAL_SERVER_ERROR,
              }),
            );
          }
        } finally {
          await awaitPendingTimers();
        }
      },
    },
    initEndpoint: {
      init: function (): InitResult {
        return init;
      },
    },
  };
}

function writeResponse(
  responseOutparam: ResponseOutparam,
  response: OutgoingResponse,
) {
  ResponseOutparam.set(responseOutparam, { tag: "ok", val: response });
}
