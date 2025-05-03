export { fs } from "./deno";

// Redirect console output to stderr, to keep stdout for request logs.
declare global {
  var Deno: {
    core: {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      print: any;
    };
  };
}
const _logStderr = function (...args: unknown[]) {
  globalThis.Deno.core.print(`${args.join(" ")}\n`, /* to stderr = */ true);
};
globalThis.console.log = _logStderr;
globalThis.console.info = _logStderr;
globalThis.console.debug = _logStderr;

export {
  HttpError,
  StatusCodes,
  addCronCallback,
  addPeriodicCallback,
  addRoute,
  execute,
  htmlHandler,
  jsonHandler,
  parsePath,
  query,
  stringHandler,
  transaction,
  Transaction,
} from "./trailbase";

export type {
  CallbackType,
  HeaderMapType,
  HtmlResponseType,
  JsonRequestType,
  JsonResponseType,
  MaybeResponse,
  Method,
  ParsedPath,
  PathParamsType,
  RequestType,
  ResponseType,
  StringRequestType,
  StringResponseType,
  UserType,
} from "./trailbase";
