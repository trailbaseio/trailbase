import {
  IncomingRequest,
  OutgoingResponse,
  ResponseOutparam,
  Method as WasiMethod,
} from "wasi:http/types@0.2.3";

import type { HttpContext } from "@common/HttpContext";
import type { HttpHandlerInterface, ResponseType } from "./index";
import { StatusCode } from "./index";
import { HttpError, HttpResponse, buildResponse } from "./response";
import { type Method, HttpRequestImpl } from "./request";
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
        new HttpRequestImpl(
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
      const outgoingResp = responseToOutgoingResponse(resp);
      writeResponse(respOutparam, outgoingResp);
    } catch (err) {
      writeResponse(respOutparam, errorToOutgoingResponse(err));
    } finally {
      await awaitPendingTimers();
    }
  };
}

export function responseToOutgoingResponse(
  resp: ResponseType,
): OutgoingResponse {
  if (resp instanceof OutgoingResponse) {
    return resp;
  } else if (resp instanceof HttpResponse) {
    return buildResponse({
      status: resp.status,
      headers: resp.headers,
      body: resp.body ?? new Uint8Array(),
    });
  } else if (resp instanceof Uint8Array) {
    return buildResponse({
      status: StatusCode.OK,
      headers: [],
      body: resp,
    });
  } else if (typeof resp === "string") {
    return buildResponse({
      status: StatusCode.OK,
      headers: [],
      body: encodeBytes(resp),
    });
  } else {
    // void case.
    return buildResponse({
      status: StatusCode.OK,
      headers: [],
      body: new Uint8Array(),
    });
  }
}

export function errorToOutgoingResponse(err: unknown): OutgoingResponse {
  if (err instanceof HttpError) {
    return buildResponse({
      status: err.status,
      headers: [["Content-Type", encodeBytes("text/plain; charset=utf-8")]],
      body: err.message ? encodeBytes(err.message) : new Uint8Array(),
    });
  }
  return buildResponse({
    body: encodeBytes(`uncaught: ${err}`),
    status: StatusCode.INTERNAL_SERVER_ERROR,
    headers: [],
  });
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
