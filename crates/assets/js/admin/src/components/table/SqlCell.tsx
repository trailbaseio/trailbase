import { JSX } from "solid-js";
import type { CellContext } from "@tanstack/solid-table";
import { urlSafeBase64Decode } from "trailbase";

import { wkbToWkt } from "@/lib/geometry";
import { toHex } from "@/lib/utils";
import type { ArrayRecord } from "@/lib/record";
import {
  getForeignKey,
  isFileUploadColumn,
  isFileUploadsColumn,
  isGeometryColumn,
  isJSONColumn,
  isNotNull,
  isUUIDColumn,
} from "@/lib/schema";

import type { Column } from "@bindings/Column";
import type { ColumnDataType } from "@bindings/ColumnDataType";
import type { SqlValue } from "@bindings/SqlValue";
import type { QualifiedName } from "@bindings/QualifiedName";

import type { BlobEncoding } from "@/components/table/BlobEncoding";
import {
  type FileUpload,
  type FileUploads,
  UploadedFile,
  UploadedFiles,
} from "@/components/table/Files";
import { Uuid } from "@/components/table/Uuid";

export type CellType =
  "UUID" | "JSON" | "File" | "File[]" | "Geometry" | ColumnDataType;

export function deriveCellType(column: Column): CellType {
  if (isUUIDColumn(column)) {
    return "UUID";
  }
  if (isGeometryColumn(column)) {
    return "Geometry";
  }
  if (isFileUploadColumn(column)) {
    return "File";
  }
  if (isFileUploadsColumn(column)) {
    return "File[]";
  }

  if (isJSONColumn(column)) {
    return "JSON";
  }

  return column.data_type;
}

export function defaultHeader(column: Column): string {
  const cellType = deriveCellType(column);
  const notNull = isNotNull(column.options);
  const typeName = notNull ? cellType : `${cellType}?`;

  const fk = getForeignKey(column.options);
  const fkSuffix = fk ? ` ‣ ${fk.foreign_table}[${fk.referred_columns}]` : "";

  return `${column.name} [${typeName}] ${fkSuffix}`;
}

export function renderCell(
  context: CellContext<ArrayRecord, SqlValue>,
  column: Column,
  blobEncoding: BlobEncoding,
  fileColumnSupport?: {
    tableName: QualifiedName;
    columns: Column[];
    pkIndex: number;
    rowsRefetch: () => void;
  },
  type?: CellType,
): JSX.Element {
  const cellType = type ?? deriveCellType(column);
  const value: SqlValue = context.getValue();

  // Special handling for file columns.
  if (fileColumnSupport !== undefined) {
    const { tableName, columns, pkIndex, rowsRefetch } = fileColumnSupport;

    if (cellType === "File") {
      let file: FileUpload | null;
      if (value === "Null") {
        file = null;
      } else if ("Text" in value) {
        file = JSON.parse(value.Text) as FileUpload;
      } else {
        throw new Error("expected JSON text");
      }

      const pkCol = columns[pkIndex].name;
      const pkVal = context.row.original[pkIndex];

      return (
        <UploadedFile
          file={file}
          tableName={tableName}
          columnName={column.name}
          columns={columns}
          pk={{ columnName: pkCol, value: pkVal }}
          rowsRefetch={rowsRefetch}
        />
      );
    } else if (cellType === "File[]") {
      let files: FileUploads;
      if (value === "Null") {
        files = [];
      } else if ("Text" in value) {
        files = JSON.parse(value.Text) as FileUploads;
      } else {
        throw new Error("expected JSON text");
      }

      const pkCol = columns[pkIndex].name;
      const pkVal = context.row.original[pkIndex];

      return (
        <UploadedFiles
          files={files}
          tableName={tableName}
          columnName={column.name}
          columns={columns}
          pk={{ columnName: pkCol, value: pkVal }}
          rowsRefetch={rowsRefetch}
        />
      );
    }
  }

  if (value === "Null") {
    return "NULL";
  }

  if ("Integer" in value) {
    return value.Integer.toString();
  }

  if ("Real" in value) {
    return value.Real.toString();
  }

  if ("Blob" in value) {
    const blob = value.Blob;
    if ("Base64UrlSafe" in blob) {
      switch (cellType) {
        case "UUID": {
          return (
            <Uuid
              base64UrlSafeBlob={blob.Base64UrlSafe}
              blobEncoding={blobEncoding}
            />
          );
        }
        case "Geometry": {
          return wkbToWkt(urlSafeBase64Decode(blob.Base64UrlSafe));
        }
      }

      if (blobEncoding === "hex") {
        return toHex(urlSafeBase64Decode(blob.Base64UrlSafe));
      }
      return blob.Base64UrlSafe;
    }
    throw Error("Expected Base64UrlSafe");
  }

  if ("Text" in value) {
    return value.Text;
  }

  throw Error("Unhandled value type");
}
