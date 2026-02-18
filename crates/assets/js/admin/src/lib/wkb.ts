// Derived from: https://github.com/conveyal/osmix/blob/8cc2d43a12a722449c63c71e0d8b7d77583ca82c/packages/geoparquet/src/wkb.ts (MIT)

/**
 * WKB (Well-Known Binary) geometry parsing utilities.
 *
 * Browser-compatible WKB parser using DataView instead of Node.js Buffer.
 * Supports standard WKB and EWKB (with SRID) formats.
 *
 * @module
 */

import type {
  Geometry,
  GeometryCollection,
  LineString,
  MultiLineString,
  MultiPoint,
  MultiPolygon,
  Point,
  Polygon,
  Position,
} from "geojson";

/** WKB geometry type codes */
const WKB_POINT = 1;
const WKB_LINESTRING = 2;
const WKB_POLYGON = 3;
const WKB_MULTIPOINT = 4;
const WKB_MULTILINESTRING = 5;
const WKB_MULTIPOLYGON = 6;
const WKB_GEOMETRYCOLLECTION = 7;

/** EWKB flags */
const EWKB_SRID_FLAG = 0x20000000;
const EWKB_Z_FLAG = 0x80000000;
const EWKB_M_FLAG = 0x40000000;

/**
 * Binary reader using DataView for browser compatibility.
 */
class WkbReader {
  private view: DataView;
  private offset = 0;
  private littleEndian = true;

  constructor(data: Uint8Array) {
    // Create DataView from the Uint8Array's underlying buffer with correct offset
    this.view = new DataView(data.buffer, data.byteOffset, data.byteLength);
  }

  readByte(): number {
    const value = this.view.getUint8(this.offset);
    this.offset += 1;
    return value;
  }

  readUint32(): number {
    const value = this.view.getUint32(this.offset, this.littleEndian);
    this.offset += 4;
    return value;
  }

  readDouble(): number {
    const value = this.view.getFloat64(this.offset, this.littleEndian);
    this.offset += 8;
    return value;
  }

  setLittleEndian(littleEndian: boolean): void {
    this.littleEndian = littleEndian;
  }
}

/**
 * Parse a WKB geometry into a GeoJSON Geometry object.
 *
 * Browser-compatible implementation using DataView.
 * Supports Point, LineString, Polygon, MultiPoint, MultiLineString,
 * MultiPolygon, and GeometryCollection. Also handles EWKB with SRID.
 *
 * @param wkb - WKB-encoded geometry as Uint8Array
 * @returns Parsed GeoJSON Geometry
 * @throws Error if geometry type is unsupported
 */
export function parseWkb(wkb: Uint8Array): Geometry {
  const reader = new WkbReader(wkb);
  return parseGeometry(reader);
}

/**
 * Parse a geometry from the reader at current position.
 */
function parseGeometry(reader: WkbReader): Geometry {
  // Read byte order
  const byteOrder = reader.readByte();
  reader.setLittleEndian(byteOrder === 1);

  // Read geometry type (may include EWKB flags)
  let geometryType = reader.readUint32();

  // Handle EWKB SRID flag
  if (geometryType & EWKB_SRID_FLAG) {
    // Skip SRID (4 bytes)
    reader.readUint32();
    geometryType &= ~EWKB_SRID_FLAG;
  }

  // Check for Z/M flags and mask them out
  const hasZ = (geometryType & EWKB_Z_FLAG) !== 0;
  const hasM = (geometryType & EWKB_M_FLAG) !== 0;
  geometryType &= 0x0000ffff; // Keep only the base type

  // Determine coordinate dimensions
  const dimensions = 2 + (hasZ ? 1 : 0) + (hasM ? 1 : 0);

  switch (geometryType) {
    case WKB_POINT:
      return parsePoint(reader, dimensions);
    case WKB_LINESTRING:
      return parseLineString(reader, dimensions);
    case WKB_POLYGON:
      return parsePolygon(reader, dimensions);
    case WKB_MULTIPOINT:
      return parseMultiPoint(reader);
    case WKB_MULTILINESTRING:
      return parseMultiLineString(reader);
    case WKB_MULTIPOLYGON:
      return parseMultiPolygon(reader);
    case WKB_GEOMETRYCOLLECTION:
      return parseGeometryCollection(reader);
    default:
      throw new Error(`Unsupported WKB geometry type: ${geometryType}`);
  }
}

/**
 * Read a coordinate (lon, lat, and optionally z/m).
 * Only returns [lon, lat] for GeoJSON compatibility.
 */
function readCoordinate(reader: WkbReader, dimensions: number): Position {
  const x = reader.readDouble();
  const y = reader.readDouble();

  // Read and discard extra dimensions (Z, M)
  for (let i = 2; i < dimensions; i++) {
    reader.readDouble();
  }

  return [x, y];
}

/**
 * Read an array of coordinates.
 */
function readCoordinates(reader: WkbReader, dimensions: number): Position[] {
  const count = reader.readUint32();
  const coords: Position[] = [];
  for (let i = 0; i < count; i++) {
    coords.push(readCoordinate(reader, dimensions));
  }
  return coords;
}

function parsePoint(reader: WkbReader, dimensions: number): Point {
  const coordinates = readCoordinate(reader, dimensions);
  return { type: "Point", coordinates };
}

function parseLineString(reader: WkbReader, dimensions: number): LineString {
  const coordinates = readCoordinates(reader, dimensions);
  return { type: "LineString", coordinates };
}

function parsePolygon(reader: WkbReader, dimensions: number): Polygon {
  const numRings = reader.readUint32();
  const coordinates: Position[][] = [];
  for (let i = 0; i < numRings; i++) {
    coordinates.push(readCoordinates(reader, dimensions));
  }
  return { type: "Polygon", coordinates };
}

function parseMultiPoint(reader: WkbReader): MultiPoint {
  const numPoints = reader.readUint32();
  const coordinates: Position[] = [];
  for (let i = 0; i < numPoints; i++) {
    const point = parseGeometry(reader) as Point;
    coordinates.push(point.coordinates);
  }
  return { type: "MultiPoint", coordinates };
}

function parseMultiLineString(reader: WkbReader): MultiLineString {
  const numLineStrings = reader.readUint32();
  const coordinates: Position[][] = [];
  for (let i = 0; i < numLineStrings; i++) {
    const lineString = parseGeometry(reader) as LineString;
    coordinates.push(lineString.coordinates);
  }
  return { type: "MultiLineString", coordinates };
}

function parseMultiPolygon(reader: WkbReader): MultiPolygon {
  const numPolygons = reader.readUint32();
  const coordinates: Position[][][] = [];
  for (let i = 0; i < numPolygons; i++) {
    const polygon = parseGeometry(reader) as Polygon;
    coordinates.push(polygon.coordinates);
  }
  return { type: "MultiPolygon", coordinates };
}

function parseGeometryCollection(reader: WkbReader): GeometryCollection {
  const numGeometries = reader.readUint32();
  const geometries: Geometry[] = [];
  for (let i = 0; i < numGeometries; i++) {
    geometries.push(parseGeometry(reader));
  }
  return { type: "GeometryCollection", geometries };
}
