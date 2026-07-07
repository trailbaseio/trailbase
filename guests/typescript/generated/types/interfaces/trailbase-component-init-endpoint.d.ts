declare module "trailbase:component/init-endpoint@0.2.0" {
  /**
   * record sqlite-scalar-function {
   *   name: string,
   *   num-args: u32,
   *   function-flags: list<sqlite-function-flags>,
   * }
   * record sqlite-functions {
   *   scalar-functions: list<sqlite-scalar-function>,
   * }
   * @since(version = 0.1.0)
   * init-sqlite-functions : func(args: arguments) -> sqlite-functions;
   * The main method to get a WASM component's manifest, self-describing what
   * it needs and what it provides.
   *
   * NOTE: The inputs and outputs are opaque JSON (maybe Avro in the the
   * future), since WIT type's versioning capabilities are insufficient to
   * evolve this API w/o constantly breaking otherwise perfectly fine
   * combinations of client and server.
   */
  export function getManifest(args: string): string;
  /**
   * record arguments {
   *   version: option<string>,
   * }
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
  export type HttpMethodType =
    | "get"
    | "post"
    | "head"
    | "options"
    | "patch"
    | "delete"
    | "put"
    | "trace"
    | "connect";
  /**
   * record http-handlers {
   *   /// Registered http handlers (method, path)[].
   *   handlers: list<tuple<http-method-type, string>>,
   * }
   * @since(version = 0.1.0)
   * init-http-handlers: func(args: arguments) -> http-handlers;
   * record job-handlers {
   *   /// Registered jobs (name, spec)[].
   *   handlers: list<tuple<string, string>>,
   * }
   * @since(version = 0.1.0)
   * init-job-handlers: func(args: arguments) -> job-handlers;
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
  export type SqliteFunctionFlags =
    | "utf8"
    | "utf16le"
    | "utf16be"
    | "utf16"
    | "deterministic"
    | "direct-only"
    | "subtype"
    | "innocuous"
    | "result-subtype"
    | "selforder1";
}
