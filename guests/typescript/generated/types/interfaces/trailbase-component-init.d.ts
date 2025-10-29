declare module 'trailbase:component/init@0.1.0' {
  export function initHttpHandlers(args: Arguments): HttpHandlers;
  export function initJobHandlers(args: Arguments): JobHandlers;
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
}
