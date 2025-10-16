/* eslint-disable no-fallthrough */

// From: https://github.com/ungap/raw-json/blob/main/index.js
let { parse, stringify, rawJSON, isRawJSON } = JSON;

if (!rawJSON) {
  // isRawJSON
  const { freeze, setPrototypeOf } = Object;
  class RawJSON {
    static isRawJSON(o) {
      return typeof o === "object" && !!o && #rawJSON in o;
    }
    #rawJSON = true;
    constructor(rawJSON) {
      this.rawJSON = rawJSON;
      freeze(setPrototypeOf(this, null));
    }
  }
  ({ isRawJSON } = RawJSON);

  // rawJSON
  rawJSON = (value) => {
    const type = typeof value;
    switch (type) {
      case "object":
        if (value) break;
      case "bigint":
      case "boolean":
      case "number":
        value = String(value);
      case "string":
        parse(value);
        return new RawJSON(value);
    }
    throw new TypeError("Unexpected " + type);
  };

  // parse
  const p = parse;
  parse = (text, reviver) => {
    if (typeof reviver === "function") {
      const $ = reviver;
      const a = [];
      text = String(text).replace(
        // parse all primitives in one go: string | number | boolean | null
        /(?:"(?:(?=(\\?))\1.)*?"\s*:?|[0-9eE+.-]+|true|false|null)/g,
        ($) =>
          $.at(-1) !== ":" ? ($[0] === '"' ? a.push($) - 1 : `"${$}"`) : $,
      );
      reviver = function (key, source) {
        switch (typeof source) {
          // abracadabra
          case "number":
            source = a[source];
          case "string":
            return $.call(this, key, p(source), { source });
        }
        return $.call(this, key, source, {});
      };
    }
    return p(text, reviver);
  };

  // stringify
  const { isArray } = Array;
  const noop = (_, value) => value;
  const fix = (r, raws) => {
    let id = 0;
    const suffix = `${Math.random()}00`.slice(2);
    const $ =
      typeof r === "function"
        ? r
        : r && isArray(r)
          ? (k, v) => (!k || r.includes(k) ? v : void 0)
          : noop;
    return function (key, value) {
      value = $.call(this, key, value);
      if (isRawJSON(value)) {
        const { rawJSON } = value;
        raws.set((value = `'${++id}'${suffix}`), rawJSON);
      }
      return value;
    };
  };
  const s = stringify;
  stringify = (value, replacer, space) => {
    const m = new Map(),
      t = s(value, fix(replacer, m), space);
    return m.size
      ? t.replace(new RegExp(`"(${[...m.keys()].join("|")})"`, "g"), (_, k) =>
          m.get(k),
        )
      : t;
  };
}

export const reviver = (_, value, context) =>
  typeof value === "number" && String(value) !== context?.source
    ? BigInt(context.source)
    : value;

export { parse, stringify, rawJSON, isRawJSON };
