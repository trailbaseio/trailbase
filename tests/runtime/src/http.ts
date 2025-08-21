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

export type { InitResult } from "trailbase:runtime/init-endpoint";
export { StatusCode } from "./status";

export type Request = {
  method: Method;

  path: string;
  params: [string, string][];

  scheme: Scheme | undefined;
  authority: string;

  headers: Headers;

  user: HttpContextUser | undefined;
  body: IncomingBody;
};

type ResponseType = string | Uint8Array;
type HttpHandler = {
  path: string;
  method: MethodType;
  handler: (req: Request) => ResponseType | Promise<ResponseType>;
};

type JobHandler = {
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
  httpHandlers?: HttpHandler[];
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

      return await handler({
        method: req.method(),
        path: req.pathWithQuery() ?? "",
        params: context.path_params,
        scheme: req.scheme(),
        authority: req.authority() ?? "",
        headers: req.headers(),
        user: context.user,
        body: req.consume(),
      });
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
            StatusCode.OK,
            resp instanceof Uint8Array ? resp : encodeBytes(resp),
          );
        } catch (err) {
          if (err instanceof HttpError) {
            return writeResponse(
              respOutparam,
              err.statusCode,
              encodeBytes(err.message),
            );
          }

          return writeResponse(
            respOutparam,
            StatusCode.INTERNAL_SERVER_ERROR,
            encodeBytes(`Caught: ${err}`),
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

export function writeResponse(
  responseOutparam: ResponseOutparam,
  status: number,
  body: Uint8Array,
) {
  const outgoingResponse = new OutgoingResponse(new Fields());
  let outgoingBody = outgoingResponse.body();
  {
    // Create a stream for the response body
    let outputStream = outgoingBody.write();
    outputStream.blockingWriteAndFlush(body);
    // @ts-ignore: This is required in order to dispose the stream before we return
    outputStream[Symbol.dispose]();
    //outputStream[Symbol.dispose]?.();
  }

  outgoingResponse.setStatusCode(status);
  OutgoingBody.finish(outgoingBody, undefined);
  ResponseOutparam.set(responseOutparam, { tag: "ok", val: outgoingResponse });
}

type HttpContextUser = {
  id: string;
  email: string;
  csrf_token: string;
};

// TODO: We could auto-generate this from the common crate.
type HttpContext = {
  kind: "Http" | "Job";
  registered_path: string;
  path_params: [string, string][];
  user: HttpContextUser | undefined;
};

export function encodeBytes(body: string): Uint8Array {
  return new Uint8Array(new TextEncoder().encode(body));
}
