import { OutgoingResponse } from "wasi:http/types@0.2.3";
import { HttpRequest } from "./request";
import type { Method } from "./request";
import { HttpResponse } from "./response";

// Override setInterval/setTimeout.
import "../timer";

// Exports:
export { OutgoingResponse } from "wasi:http/types@0.2.3";
export { StatusCode } from "./status";
export type { Method, HttpRequest, Scheme, User } from "./request";
export { HttpResponse, HttpError } from "./response";

export type ResponseType =
  | string
  | Uint8Array
  | HttpResponse
  | OutgoingResponse
  | void;

export type HttpHandlerCallback = (
  req: HttpRequest,
) => ResponseType | Promise<ResponseType>;

export interface HttpHandlerInterface {
  path: string;
  method: Method;
  handler: HttpHandlerCallback;
}

export class HttpHandler implements HttpHandlerInterface {
  constructor(
    public readonly path: string,
    public readonly method: Method,
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
