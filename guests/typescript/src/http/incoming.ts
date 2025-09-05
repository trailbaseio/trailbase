import {
  IncomingRequest,
  OutgoingResponse,
  ResponseOutparam,
  Method as WasiMethod,
} from "wasi:http/types@0.2.3";

import type { HttpContext } from "@common/HttpContext";
import type { HttpHandlerInterface, ResponseType } from "./index";
import { HttpError, StatusCode, buildResponse } from "./index";
import { type Method, RequestImpl } from "./request";
import { JobHandlerInterface } from "../job";
import { awaitPendingTimers } from "../timer";

type IncomingHandler = (
  req: IncomingRequest,
  respOutparam: ResponseOutparam,
) => Promise<void>;

export function encodeBytes(body: string): Uint8Array {
  return new TextEncoder().encode(body);
}

export function buildIncomingHttpHandler(args: {
  httpHandlers?: HttpHandlerInterface[];
  jobHandlers?: JobHandlerInterface[];
}): IncomingHandler {
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
          wasiMethodToMethod(req.method()),
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

  return async function (req: IncomingRequest, respOutparam: ResponseOutparam) {
    try {
      const resp: ResponseType = await handle(req);
      writeResponse(
        respOutparam,
        resp instanceof OutgoingResponse
          ? resp
          : buildResponse(
              resp instanceof Uint8Array ? resp : encodeBytes(resp ?? ""),
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
  };
}

function writeResponse(
  responseOutparam: ResponseOutparam,
  response: OutgoingResponse,
) {
  ResponseOutparam.set(responseOutparam, { tag: "ok", val: response });
}

function wasiMethodToMethod(method: WasiMethod): Method {
  switch (method.tag) {
    case "other":
      throw new HttpError(StatusCode.INTERNAL_SERVER_ERROR, "other method");
    default:
      return method.tag;
  }
}
