import { Parser, GeometryBase } from "@tiledb-inc/wkx";

export function wkbToWkt(blob: Uint8Array): string {
  const geometry = Parser.parseWkb(new DataView(blob.buffer));

  // NOTE: Otherwise EWKT will show a leading "SRID=undefined".
  if (geometry.srid) {
    return geometry.toEwkt();
  }
  return geometry.toWkt();
}

export function wktToWkb(wkt: string): GeometryBase {
  return Parser.parseWkt(wkt);
}
