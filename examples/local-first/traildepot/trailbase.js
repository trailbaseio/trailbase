var __defProp = Object.defineProperty;
var __defNormalProp = (obj, key, value) => key in obj ? __defProp(obj, key, { enumerable: true, configurable: true, writable: true, value }) : obj[key] = value;
var __publicField = (obj, key, value) => __defNormalProp(obj, typeof key !== "symbol" ? key + "" : key, value);
var fs;
((fs2) => {
  fs2.writeFile = Deno.writeFile;
  fs2.writeTextFile = Deno.writeTextFile;
  fs2.readTextFile = Deno.readTextFile;
  fs2.readFile = Deno.readFile;
  fs2.chmod = Deno.chmod;
  fs2.chown = Deno.chown;
  fs2.cwd = Deno.cwd;
  fs2.makeTempDir = Deno.makeTempDir;
  fs2.makeTempFile = Deno.makeTempFile;
  fs2.mkdir = Deno.mkdir;
  fs2.chdir = Deno.chdir;
  fs2.copyFile = Deno.copyFile;
  fs2.readDir = Deno.readDir;
  fs2.readLink = Deno.readLink;
  fs2.realPath = Deno.realPath;
  fs2.remove = Deno.remove;
  fs2.rename = Deno.rename;
  fs2.stat = Deno.stat;
  fs2.lstat = Deno.lstat;
  fs2.truncate = Deno.truncate;
  fs2.FsFile = Deno.FsFile;
  fs2.open = Deno.open;
  fs2.create = Deno.create;
  fs2.symlink = Deno.symlink;
  fs2.link = Deno.link;
  fs2.utime = Deno.utime;
  fs2.umask = Deno.umask;
})(fs || (fs = {}));
function decodeFallback(bytes) {
  var inputIndex = 0;
  var pendingSize = Math.min(256 * 256, bytes.length + 1);
  var pending = new Uint16Array(pendingSize);
  var chunks = [];
  var pendingIndex = 0;
  for (; ; ) {
    var more = inputIndex < bytes.length;
    if (!more || pendingIndex >= pendingSize - 1) {
      var subarray = pending.subarray(0, pendingIndex);
      var arraylike = subarray;
      chunks.push(String.fromCharCode.apply(null, arraylike));
      if (!more) {
        return chunks.join("");
      }
      bytes = bytes.subarray(inputIndex);
      inputIndex = 0;
      pendingIndex = 0;
    }
    var byte1 = bytes[inputIndex++];
    if ((byte1 & 128) === 0) {
      pending[pendingIndex++] = byte1;
    } else if ((byte1 & 224) === 192) {
      var byte2 = bytes[inputIndex++] & 63;
      pending[pendingIndex++] = (byte1 & 31) << 6 | byte2;
    } else if ((byte1 & 240) === 224) {
      var byte2 = bytes[inputIndex++] & 63;
      var byte3 = bytes[inputIndex++] & 63;
      pending[pendingIndex++] = (byte1 & 31) << 12 | byte2 << 6 | byte3;
    } else if ((byte1 & 248) === 240) {
      var byte2 = bytes[inputIndex++] & 63;
      var byte3 = bytes[inputIndex++] & 63;
      var byte4 = bytes[inputIndex++] & 63;
      var codepoint = (byte1 & 7) << 18 | byte2 << 12 | byte3 << 6 | byte4;
      if (codepoint > 65535) {
        codepoint -= 65536;
        pending[pendingIndex++] = codepoint >>> 10 & 1023 | 55296;
        codepoint = 56320 | codepoint & 1023;
      }
      pending[pendingIndex++] = codepoint;
    } else ;
  }
}
function encodeFallback(string) {
  var pos = 0;
  var len = string.length;
  var at = 0;
  var tlen = Math.max(32, len + (len >>> 1) + 7);
  var target = new Uint8Array(tlen >>> 3 << 3);
  while (pos < len) {
    var value = string.charCodeAt(pos++);
    if (value >= 55296 && value <= 56319) {
      if (pos < len) {
        var extra = string.charCodeAt(pos);
        if ((extra & 64512) === 56320) {
          ++pos;
          value = ((value & 1023) << 10) + (extra & 1023) + 65536;
        }
      }
      if (value >= 55296 && value <= 56319) {
        continue;
      }
    }
    if (at + 4 > target.length) {
      tlen += 8;
      tlen *= 1 + pos / string.length * 2;
      tlen = tlen >>> 3 << 3;
      var update = new Uint8Array(tlen);
      update.set(target);
      target = update;
    }
    if ((value & 4294967168) === 0) {
      target[at++] = value;
      continue;
    } else if ((value & 4294965248) === 0) {
      target[at++] = value >>> 6 & 31 | 192;
    } else if ((value & 4294901760) === 0) {
      target[at++] = value >>> 12 & 15 | 224;
      target[at++] = value >>> 6 & 63 | 128;
    } else if ((value & 4292870144) === 0) {
      target[at++] = value >>> 18 & 7 | 240;
      target[at++] = value >>> 12 & 63 | 128;
      target[at++] = value >>> 6 & 63 | 128;
    } else {
      continue;
    }
    target[at++] = value & 63 | 128;
  }
  return target.slice ? target.slice(0, at) : target.subarray(0, at);
}
var StatusCodes = /* @__PURE__ */ ((StatusCodes2) => {
  StatusCodes2[StatusCodes2["CONTINUE"] = 100] = "CONTINUE";
  StatusCodes2[StatusCodes2["SWITCHING_PROTOCOLS"] = 101] = "SWITCHING_PROTOCOLS";
  StatusCodes2[StatusCodes2["PROCESSING"] = 102] = "PROCESSING";
  StatusCodes2[StatusCodes2["EARLY_HINTS"] = 103] = "EARLY_HINTS";
  StatusCodes2[StatusCodes2["OK"] = 200] = "OK";
  StatusCodes2[StatusCodes2["CREATED"] = 201] = "CREATED";
  StatusCodes2[StatusCodes2["ACCEPTED"] = 202] = "ACCEPTED";
  StatusCodes2[StatusCodes2["NON_AUTHORITATIVE_INFORMATION"] = 203] = "NON_AUTHORITATIVE_INFORMATION";
  StatusCodes2[StatusCodes2["NO_CONTENT"] = 204] = "NO_CONTENT";
  StatusCodes2[StatusCodes2["RESET_CONTENT"] = 205] = "RESET_CONTENT";
  StatusCodes2[StatusCodes2["PARTIAL_CONTENT"] = 206] = "PARTIAL_CONTENT";
  StatusCodes2[StatusCodes2["MULTI_STATUS"] = 207] = "MULTI_STATUS";
  StatusCodes2[StatusCodes2["MULTIPLE_CHOICES"] = 300] = "MULTIPLE_CHOICES";
  StatusCodes2[StatusCodes2["MOVED_PERMANENTLY"] = 301] = "MOVED_PERMANENTLY";
  StatusCodes2[StatusCodes2["MOVED_TEMPORARILY"] = 302] = "MOVED_TEMPORARILY";
  StatusCodes2[StatusCodes2["SEE_OTHER"] = 303] = "SEE_OTHER";
  StatusCodes2[StatusCodes2["NOT_MODIFIED"] = 304] = "NOT_MODIFIED";
  StatusCodes2[StatusCodes2["USE_PROXY"] = 305] = "USE_PROXY";
  StatusCodes2[StatusCodes2["TEMPORARY_REDIRECT"] = 307] = "TEMPORARY_REDIRECT";
  StatusCodes2[StatusCodes2["PERMANENT_REDIRECT"] = 308] = "PERMANENT_REDIRECT";
  StatusCodes2[StatusCodes2["BAD_REQUEST"] = 400] = "BAD_REQUEST";
  StatusCodes2[StatusCodes2["UNAUTHORIZED"] = 401] = "UNAUTHORIZED";
  StatusCodes2[StatusCodes2["PAYMENT_REQUIRED"] = 402] = "PAYMENT_REQUIRED";
  StatusCodes2[StatusCodes2["FORBIDDEN"] = 403] = "FORBIDDEN";
  StatusCodes2[StatusCodes2["NOT_FOUND"] = 404] = "NOT_FOUND";
  StatusCodes2[StatusCodes2["METHOD_NOT_ALLOWED"] = 405] = "METHOD_NOT_ALLOWED";
  StatusCodes2[StatusCodes2["NOT_ACCEPTABLE"] = 406] = "NOT_ACCEPTABLE";
  StatusCodes2[StatusCodes2["PROXY_AUTHENTICATION_REQUIRED"] = 407] = "PROXY_AUTHENTICATION_REQUIRED";
  StatusCodes2[StatusCodes2["REQUEST_TIMEOUT"] = 408] = "REQUEST_TIMEOUT";
  StatusCodes2[StatusCodes2["CONFLICT"] = 409] = "CONFLICT";
  StatusCodes2[StatusCodes2["GONE"] = 410] = "GONE";
  StatusCodes2[StatusCodes2["LENGTH_REQUIRED"] = 411] = "LENGTH_REQUIRED";
  StatusCodes2[StatusCodes2["PRECONDITION_FAILED"] = 412] = "PRECONDITION_FAILED";
  StatusCodes2[StatusCodes2["REQUEST_TOO_LONG"] = 413] = "REQUEST_TOO_LONG";
  StatusCodes2[StatusCodes2["REQUEST_URI_TOO_LONG"] = 414] = "REQUEST_URI_TOO_LONG";
  StatusCodes2[StatusCodes2["UNSUPPORTED_MEDIA_TYPE"] = 415] = "UNSUPPORTED_MEDIA_TYPE";
  StatusCodes2[StatusCodes2["REQUESTED_RANGE_NOT_SATISFIABLE"] = 416] = "REQUESTED_RANGE_NOT_SATISFIABLE";
  StatusCodes2[StatusCodes2["EXPECTATION_FAILED"] = 417] = "EXPECTATION_FAILED";
  StatusCodes2[StatusCodes2["IM_A_TEAPOT"] = 418] = "IM_A_TEAPOT";
  StatusCodes2[StatusCodes2["INSUFFICIENT_SPACE_ON_RESOURCE"] = 419] = "INSUFFICIENT_SPACE_ON_RESOURCE";
  StatusCodes2[StatusCodes2["METHOD_FAILURE"] = 420] = "METHOD_FAILURE";
  StatusCodes2[StatusCodes2["MISDIRECTED_REQUEST"] = 421] = "MISDIRECTED_REQUEST";
  StatusCodes2[StatusCodes2["UNPROCESSABLE_ENTITY"] = 422] = "UNPROCESSABLE_ENTITY";
  StatusCodes2[StatusCodes2["LOCKED"] = 423] = "LOCKED";
  StatusCodes2[StatusCodes2["FAILED_DEPENDENCY"] = 424] = "FAILED_DEPENDENCY";
  StatusCodes2[StatusCodes2["UPGRADE_REQUIRED"] = 426] = "UPGRADE_REQUIRED";
  StatusCodes2[StatusCodes2["PRECONDITION_REQUIRED"] = 428] = "PRECONDITION_REQUIRED";
  StatusCodes2[StatusCodes2["TOO_MANY_REQUESTS"] = 429] = "TOO_MANY_REQUESTS";
  StatusCodes2[StatusCodes2["REQUEST_HEADER_FIELDS_TOO_LARGE"] = 431] = "REQUEST_HEADER_FIELDS_TOO_LARGE";
  StatusCodes2[StatusCodes2["UNAVAILABLE_FOR_LEGAL_REASONS"] = 451] = "UNAVAILABLE_FOR_LEGAL_REASONS";
  StatusCodes2[StatusCodes2["INTERNAL_SERVER_ERROR"] = 500] = "INTERNAL_SERVER_ERROR";
  StatusCodes2[StatusCodes2["NOT_IMPLEMENTED"] = 501] = "NOT_IMPLEMENTED";
  StatusCodes2[StatusCodes2["BAD_GATEWAY"] = 502] = "BAD_GATEWAY";
  StatusCodes2[StatusCodes2["SERVICE_UNAVAILABLE"] = 503] = "SERVICE_UNAVAILABLE";
  StatusCodes2[StatusCodes2["GATEWAY_TIMEOUT"] = 504] = "GATEWAY_TIMEOUT";
  StatusCodes2[StatusCodes2["HTTP_VERSION_NOT_SUPPORTED"] = 505] = "HTTP_VERSION_NOT_SUPPORTED";
  StatusCodes2[StatusCodes2["INSUFFICIENT_STORAGE"] = 507] = "INSUFFICIENT_STORAGE";
  StatusCodes2[StatusCodes2["NETWORK_AUTHENTICATION_REQUIRED"] = 511] = "NETWORK_AUTHENTICATION_REQUIRED";
  return StatusCodes2;
})(StatusCodes || {});
class HttpError extends Error {
  constructor(statusCode, message, headers) {
    super(message);
    __publicField(this, "statusCode");
    __publicField(this, "headers");
    this.statusCode = statusCode;
    this.headers = headers;
  }
  toString() {
    return `HttpError(${this.statusCode}, ${this.message})`;
  }
  toResponse() {
    const m = this.message;
    return {
      headers: this.headers,
      status: this.statusCode,
      body: m !== "" ? encodeFallback(m) : void 0
    };
  }
}
function stringHandler(f) {
  return async (req) => {
    try {
      const body = req.body;
      const resp = await f({
        uri: req.uri,
        params: req.params,
        headers: req.headers,
        user: req.user,
        body: body && decodeFallback(body)
      });
      if (resp === void 0) {
        return void 0;
      }
      if (typeof resp === "string") {
        return {
          status: 200,
          body: encodeFallback(resp)
        };
      }
      const respBody = resp.body;
      return {
        headers: resp.headers,
        status: resp.status,
        body: respBody ? encodeFallback(respBody) : void 0
      };
    } catch (err) {
      if (err instanceof HttpError) {
        return err.toResponse();
      }
      return {
        status: 500,
        body: encodeFallback(`Uncaught error: ${err}`)
      };
    }
  };
}
function htmlHandler(f) {
  return async (req) => {
    try {
      const body = req.body;
      const resp = await f({
        uri: req.uri,
        params: req.params,
        headers: req.headers,
        user: req.user,
        body: body && decodeFallback(body)
      });
      if (resp === void 0) {
        return void 0;
      }
      if (typeof resp === "string") {
        return {
          headers: [["content-type", "text/html"]],
          status: 200,
          body: encodeFallback(resp)
        };
      }
      const respBody = resp.body;
      return {
        headers: [["content-type", "text/html"], ...resp.headers ?? []],
        status: resp.status,
        body: respBody ? encodeFallback(respBody) : void 0
      };
    } catch (err) {
      if (err instanceof HttpError) {
        return err.toResponse();
      }
      return {
        status: 500,
        body: encodeFallback(`Uncaught error: ${err}`)
      };
    }
  };
}
function jsonHandler(f) {
  return async (req) => {
    try {
      const body = req.body;
      const resp = await f({
        uri: req.uri,
        params: req.params,
        headers: req.headers,
        user: req.user,
        body: body && decodeFallback(body)
      });
      if (resp === void 0) {
        return void 0;
      }
      if ("body" in resp) {
        const r = resp;
        const rBody = r.body;
        return {
          headers: [["content-type", "application/json"], ...r.headers ?? []],
          status: r.status,
          body: rBody ? encodeFallback(JSON.stringify(rBody)) : void 0
        };
      }
      return {
        headers: [["content-type", "application/json"]],
        status: 200,
        body: encodeFallback(JSON.stringify(resp))
      };
    } catch (err) {
      if (err instanceof HttpError) {
        return err.toResponse();
      }
      return {
        headers: [["content-type", "application/json"]],
        status: 500,
        body: encodeFallback(`Uncaught error: ${err}`)
      };
    }
  };
}
const routerCallbacks = /* @__PURE__ */ new Map();
function isolateId() {
  return rustyscript.functions.isolate_id();
}
function addRoute(method, route, callback) {
  if (isolateId() === 0) {
    rustyscript.functions.install_route(method, route);
    console.debug("JS: Added route:", method, route);
  }
  routerCallbacks.set(`${method}:${route}`, callback);
}
async function dispatch(method, route, uri, pathParams, headers, user, body) {
  const key = `${method}:${route}`;
  const cb = routerCallbacks.get(key);
  if (!cb) {
    throw Error(`Missing callback: ${key}`);
  }
  return await cb({
    uri,
    params: Object.fromEntries(pathParams),
    headers: Object.fromEntries(headers),
    user,
    body
  }) ?? {
    status: 200
    /* OK */
  };
}
globalThis.__dispatch = dispatch;
const cronCallbacks = /* @__PURE__ */ new Map();
function addCronCallback(name, schedule, cb) {
  const cronRegex = /^(@(yearly|monthly|weekly|daily|hourly|))|((((\d+,)+\d+|(\d+(\/|-)\d+)|\d+|\*)\s*){6,7})$/;
  const matches = cronRegex.test(schedule);
  if (!matches) {
    throw Error(`Not a valid 6/7-component cron schedule: ${schedule}`);
  }
  if (isolateId() === 0) {
    const id = rustyscript.functions.install_job(name, schedule);
    console.debug(`JS: Added cron job (id=${id}): "${name}"`);
    cronCallbacks.set(id, cb);
  }
}
async function dispatchCron(id) {
  const cb = cronCallbacks.get(id);
  if (!cb) {
    throw Error(`Missing cron callback: ${id}`);
  }
  try {
    await cb();
  } catch (err) {
    return `${err}`;
  }
}
globalThis.__dispatchCron = dispatchCron;
function addPeriodicCallback(milliseconds, cb) {
  if (isolateId() !== 0) {
    return () => {
    };
  }
  const handle = setInterval(() => {
    cb(() => clearInterval(handle));
  }, milliseconds);
  return () => clearInterval(handle);
}
async function query(sql, params) {
  return await rustyscript.async_functions.query(sql, params);
}
async function execute(sql, params) {
  return await rustyscript.async_functions.execute(sql, params);
}
class Transaction {
  constructor() {
    __publicField(this, "finalized");
    this.finalized = false;
  }
  query(queryStr, params) {
    return rustyscript.functions.transaction_query(queryStr, params);
  }
  execute(queryStr, params) {
    return rustyscript.functions.transaction_execute(queryStr, params);
  }
  commit() {
    this.finalized = true;
    rustyscript.functions.transaction_commit();
  }
  rollback() {
    this.finalized = true;
    rustyscript.functions.transaction_rollback();
  }
}
async function transaction(f) {
  await rustyscript.async_functions.transaction_begin();
  const tx = new Transaction();
  try {
    const r = f(tx);
    if (!tx.finalized) {
      rustyscript.functions.transaction_rollback();
    }
    return r;
  } catch (e) {
    rustyscript.functions.transaction_rollback();
    throw e;
  }
}
function parsePath(path) {
  const queryIndex = path.indexOf("?");
  if (queryIndex >= 0) {
    return {
      path: path.slice(0, queryIndex),
      query: new URLSearchParams(path.slice(queryIndex + 1))
    };
  }
  return {
    path,
    query: new URLSearchParams()
  };
}
const _logStderr = function(...args) {
  globalThis.Deno.core.print(
    `${args.join(" ")}
`,
    /* to stderr = */
    true
  );
};
globalThis.console.log = _logStderr;
globalThis.console.info = _logStderr;
globalThis.console.debug = _logStderr;
export {
  HttpError,
  StatusCodes,
  Transaction,
  addCronCallback,
  addPeriodicCallback,
  addRoute,
  execute,
  fs,
  htmlHandler,
  jsonHandler,
  parsePath,
  query,
  stringHandler,
  transaction
};
