import {
  Fields,
  Headers,
  IncomingBody,
  IncomingRequest,
  Method,
  OutgoingBody,
  OutgoingResponse,
  ResponseOutparam,
  Scheme,
} from "wasi:http/types@0.2.3";
import type { InitResult, MethodType } from "trailbase:runtime/init-endpoint";

import { StatusCode } from "./status";
import type { HttpContext } from "@common/HttpContext";
import type { HttpContextUser } from "@common/HttpContextUser";

export { OutgoingResponse } from "wasi:http/types@0.2.3";
export type { InitResult } from "trailbase:runtime/init-endpoint";
export { StatusCode } from "./status";

function printScheme(scheme: Scheme): string {
  switch (scheme.tag) {
    case "HTTP":
      return "http";
    case "HTTPS":
      return "https";
    case "other":
      return scheme.val;
  }
}

export class Request {
  constructor(
    public readonly method: Method,
    public readonly path: string,
    // Path params, e.g. /{placeholder}/test.
    public readonly params: [string, string][],
    public readonly scheme: Scheme | undefined,
    public readonly authority: string,
    public readonly headers: Headers,
    public readonly user: HttpContextUser | null,
    public readonly body: IncomingBody,
  ) {}

  url(): URL {
    const base =
      this.scheme !== undefined
        ? `${printScheme(this.scheme)}://${this.authority}/${this.path}`
        : `/${this.path}`;
    return new URL(base);
  }

  getQueryParam(param: string): string | null {
    return this.url().searchParams.get(param);
  }

  getPathParam(param: string): string | null {
    for (const [p, v] of this.params) {
      if (p === param) return v;
    }
    return null;
  }
}

export type ResponseType = string | Uint8Array | OutgoingResponse;
export type HttpHandlerCallback = (
  req: Request,
) => ResponseType | Promise<ResponseType>;

export interface HttpHandlerInterface {
  path: string;
  method: MethodType;
  handler: HttpHandlerCallback;
}

export class HttpHandler implements HttpHandlerInterface {
  constructor(
    public readonly path: string,
    public readonly method: MethodType,
    public readonly handler: HttpHandlerCallback,
  ) {}

  static get(path: string, handler: HttpHandlerCallback): HttpHandler {
    return new HttpHandler(path, "get", handler);
  }

  static post(path: string, handler: HttpHandlerCallback): HttpHandler {
    return new HttpHandler(path, "post", handler);
  }

  static head(path: string, handler: HttpHandlerCallback): HttpHandler {
    return new HttpHandler(path, "head", handler);
  }

  static options(path: string, handler: HttpHandlerCallback): HttpHandler {
    return new HttpHandler(path, "options", handler);
  }

  static patch(path: string, handler: HttpHandlerCallback): HttpHandler {
    return new HttpHandler(path, "patch", handler);
  }

  static delete(path: string, handler: HttpHandlerCallback): HttpHandler {
    return new HttpHandler(path, "delete", handler);
  }

  static put(path: string, handler: HttpHandlerCallback): HttpHandler {
    return new HttpHandler(path, "put", handler);
  }
}

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

export class HttpError extends Error {
  readonly statusCode: number;
  readonly headers: [string, string][] | undefined;

  constructor(
    statusCode: number,
    message?: string,
    headers?: [string, string][],
  ) {
    super(message);
    this.statusCode = statusCode;
    this.headers = headers;
  }

  public override toString(): string {
    return `HttpError(${this.statusCode}, ${this.message})`;
  }
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
        new Request(
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
          return writeResponse(
            respOutparam,
            resp instanceof OutgoingResponse
              ? resp
              : buildResponse(
                  resp instanceof Uint8Array ? resp : encodeBytes(resp),
                ),
          );
        } catch (err) {
          if (err instanceof HttpError) {
            return writeResponse(
              respOutparam,
              buildResponse(encodeBytes(err.message), {
                status: err.statusCode,
              }),
            );
          }

          return writeResponse(
            respOutparam,
            buildResponse(encodeBytes(`Caught: ${err}`), {
              status: StatusCode.INTERNAL_SERVER_ERROR,
            }),
          );
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

type ResponseOptions = {
  status?: StatusCode;
  headers?: [string, Uint8Array][];
};

export function buildJsonResponse(
  body: object,
  opts?: ResponseOptions,
): OutgoingResponse {
  return buildResponse(encodeBytes(JSON.stringify(body)), {
    ...opts,

    headers: [
      ["Content-Type", encodeBytes("application/json")],
      ...(opts?.headers ?? []),
    ],
  });
}

function buildResponse(
  body: Uint8Array,
  opts?: ResponseOptions,
): OutgoingResponse {
  const outgoingResponse = new OutgoingResponse(
    Fields.fromList(opts?.headers ?? []),
  );

  const outgoingBody = outgoingResponse.body();
  {
    // Create a stream for the response body
    const outputStream = outgoingBody.write();
    outputStream.blockingWriteAndFlush(body);

    // eslint-disable-next-line @typescript-eslint/ban-ts-comment
    // @ts-ignore: This is required in order to dispose the stream before we return
    outputStream[Symbol.dispose]();
    //outputStream[Symbol.dispose]?.();
  }

  outgoingResponse.setStatusCode(opts?.status ?? StatusCode.OK);

  OutgoingBody.finish(outgoingBody, undefined);

  return outgoingResponse;
}

// function writeResponse(
//   responseOutparam: ResponseOutparam,
//   status: number,
//   body: Uint8Array,
// ) {
//   /* eslint-disable prefer-const */
//   const outgoingResponse = new OutgoingResponse(new Fields());
//
//   let outgoingBody = outgoingResponse.body();
//   {
//     // Create a stream for the response body
//     let outputStream = outgoingBody.write();
//     outputStream.blockingWriteAndFlush(body);
//
//     // eslint-disable-next-line @typescript-eslint/ban-ts-comment
//     // @ts-ignore: This is required in order to dispose the stream before we return
//     outputStream[Symbol.dispose]();
//     //outputStream[Symbol.dispose]?.();
//   }
//
//   outgoingResponse.setStatusCode(status);
//   OutgoingBody.finish(outgoingBody, undefined);
//   ResponseOutparam.set(responseOutparam, { tag: "ok", val: outgoingResponse });
// }

function writeResponse(
  responseOutparam: ResponseOutparam,
  response: OutgoingResponse,
) {
  ResponseOutparam.set(responseOutparam, { tag: "ok", val: response });
}

export function encodeBytes(body: string): Uint8Array {
  return new Uint8Array(new TextEncoder().encode(body));
}
