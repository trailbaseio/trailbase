declare var rustyscript: any;
declare var globalThis: any;

type Headers = { [key: string]: string };
type Request = {
  uri: string;
  headers: Headers;
  body: string;
};
type Response = {
  headers?: Headers;
  status?: number;
  body?: string;
};
type CbType = (req: Request) => Response | undefined;

const callbacks = new Map<string, CbType>();

export function addRoute(method: string, route: string, callback: CbType) {
  rustyscript.functions.route(method, route);
  callbacks.set(`${method}:${route}`, callback);

  console.log("JS: Added route:", method, route);
}

export async function query(
  queryStr: string,
  params: unknown[],
): Promise<unknown[][]> {
  return await rustyscript.async_functions.query(queryStr, params);
}

export async function execute(
  queryStr: string,
  params: unknown[],
): Promise<number> {
  return await rustyscript.async_functions.execute(queryStr, params);
}

export function dispatch(
  method: string,
  route: string,
  uri: string,
  headers: Headers,
  body: string,
): Response | undefined {
  const key = `${method}:${route}`;
  const cb = callbacks.get(key);
  if (!cb) {
    throw Error(`Missing callback: ${key}`);
  }

  return cb({
    uri,
    headers,
    body,
  });
}

globalThis.__dispatch = dispatch;
