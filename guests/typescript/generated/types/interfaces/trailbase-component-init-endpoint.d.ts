declare module "trailbase:component/init-endpoint@0.2.0" {
  /**
   * The main method to get a WASM component's manifest, self-describing what
   * it needs and what it provides.
   *
   * NOTE: The inputs and outputs are opaque JSON (maybe Avro in the the
   * future), since WIT type's versioning capabilities are insufficient to
   * evolve this API w/o constantly breaking otherwise perfectly fine
   * combinations of client and server.
   */
  export function getManifest(args: string): string;
}
