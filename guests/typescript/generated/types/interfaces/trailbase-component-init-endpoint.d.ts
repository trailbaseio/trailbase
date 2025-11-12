declare module 'trailbase:component/init-endpoint@0.1.0' {
  export function initHttpHandlers(args: Arguments): HttpHandlers;
  export function initJobHandlers(args: Arguments): JobHandlers;
  export function initSqliteFunctions(args: Arguments): SqliteFunctions;
  export interface Arguments {
    version?: string,
  }
  /**
   * # Variants
   *
   * ## `"get"`
   *
   * ## `"post"`
   *
   * ## `"head"`
   *
   * ## `"options"`
   *
   * ## `"patch"`
   *
   * ## `"delete"`
   *
   * ## `"put"`
   *
   * ## `"trace"`
   *
   * ## `"connect"`
   */
  export type HttpMethodType = 'get' | 'post' | 'head' | 'options' | 'patch' | 'delete' | 'put' | 'trace' | 'connect';
  export interface HttpHandlers {
    /**
     * Registered http handlers (method, path)[].
     */
    handlers: Array<[HttpMethodType, string]>,
  }
  export interface JobHandlers {
    /**
     * Registered jobs (name, spec)[].
     */
    handlers: Array<[string, string]>,
  }
  /**
   * # Variants
   *
   * ## `"utf8"`
   *
   * Specifies UTF-8 as the text encoding this SQL function prefers for its parameters.
   * ## `"utf16le"`
   *
   * Specifies UTF-16 using little-endian byte order as the text encoding this SQL function prefers for its parameters.
   * ## `"utf16be"`
   *
   * Specifies UTF-16 using big-endian byte order as the text encoding this SQL function prefers for its parameters.
   * ## `"utf16"`
   *
   * Specifies UTF-16 using native byte order as the text encoding this SQL function prefers for its parameters.
   * ## `"deterministic"`
   *
   * Means that the function always gives the same output when the input parameters are the same.
   * ## `"direct-only"`
   *
   * Means that the function may only be invoked from top-level SQL.
   * ## `"subtype"`
   *
   * Indicates to SQLite that a function may call `sqlite3_value_subtype()` to inspect the subtypes of its arguments.
   * ## `"innocuous"`
   *
   * Means that the function is unlikely to cause problems even if misused.
   * ## `"result-subtype"`
   *
   * Indicates to SQLite that a function might call `sqlite3_result_subtype()` to cause a subtype to be associated with its result.
   * ## `"selforder1"`
   *
   * Indicates that the function is an aggregate that internally orders the values provided to the first argument.
   */
  export type SqliteFunctionFlags = 'utf8' | 'utf16le' | 'utf16be' | 'utf16' | 'deterministic' | 'direct-only' | 'subtype' | 'innocuous' | 'result-subtype' | 'selforder1';
  export interface SqliteScalarFunction {
    name: string,
    numArgs: number,
    functionFlags: Array<SqliteFunctionFlags>,
  }
  export interface SqliteFunctions {
    scalarFunctions: Array<SqliteScalarFunction>,
  }
}
