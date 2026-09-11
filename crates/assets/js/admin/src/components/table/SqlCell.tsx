import { Switch, Match, JSX } from "solid-js";
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
import type { QualifiedName } from "@bindings/QualifiedName";
import type { SqlValue } from "@bindings/SqlValue";

import type { BlobEncoding } from "@/components/table/BlobEncoding";
import {
  type FileUpload,
  type FileUploads,
  UploadedFile,
  UploadedFiles,
} from "@/components/table/Files";
import { Uuid } from "@/components/table/Uuid";
import { Link } from "@kobalte/core";
import { ForeignKey } from "@bindings/ForeignKey";

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

    const pkCol = columns[pkIndex].name;
    const pkVal = context.row.original[pkIndex];

    if (cellType === "File") {
      function contents(): FileUpload | null {
        if (value === "Null") {
          return null;
        } else if ("Text" in value) {
          return JSON.parse(value.Text) as FileUpload;
        }

        throw new Error("expected JSON text");
      }

      return (
        <UploadedFile
          file={contents()}
          tableName={tableName}
          columnName={column.name}
          columns={columns}
          pk={{ columnName: pkCol, value: pkVal }}
          rowsRefetch={rowsRefetch}
        />
      );
    } else if (cellType === "File[]") {
      function contents(): FileUploads {
        if (value === "Null") {
          return [];
        } else if ("Text" in value) {
          return JSON.parse(value.Text) as FileUploads;
        }
        throw new Error("expected JSON text");
      }

      return (
        <UploadedFiles
          files={contents()}
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
    const v = value.Integer.toString();
    return (
      <LinkForeignKey column={column} pk={v}>
        {v}
      </LinkForeignKey>
    );
  }

  if ("Real" in value) {
    return value.Real.toString();
  }

  if ("Blob" in value) {
    const blob = value.Blob;
    if (!("Base64UrlSafe" in blob)) {
      throw Error("Expected Base64UrlSafe");
    }

    switch (cellType) {
      case "UUID": {
        return (
          <LinkForeignKey column={column} pk={blob.Base64UrlSafe}>
            <Uuid
              base64UrlSafeBlob={blob.Base64UrlSafe}
              blobEncoding={blobEncoding}
            />
          </LinkForeignKey>
        );
      }
      case "Geometry": {
        return (
          <LinkForeignKey column={column} pk={blob.Base64UrlSafe}>
            {wkbToWkt(urlSafeBase64Decode(blob.Base64UrlSafe))}
          </LinkForeignKey>
        );
      }
      default: {
        return (
          <LinkForeignKey column={column} pk={blob.Base64UrlSafe}>
            {blobEncoding === "hex"
              ? toHex(urlSafeBase64Decode(blob.Base64UrlSafe))
              : blob.Base64UrlSafe}
          </LinkForeignKey>
        );
      }
    }
  }

  if ("Text" in value) {
    return (
      <LinkForeignKey column={column} pk={value.Text}>
        {value.Text}
      </LinkForeignKey>
    );
  }

  throw Error("Unhandled value type");
}

function LinkForeignKey(props: {
  pk: string;
  column: Column;
  children: JSX.Element;
}) {
  function getFkTableAndCol():
    | {
        table: string;
        col: string;
      }
    | undefined {
    const fk = getForeignKey(props.column.options);
    if (fk) {
      if (fk.referred_columns.length === 1) {
        return {
          table: fk.foreign_table,
          col: fk.referred_columns[0],
        };
      }

      if (fk.foreign_table === "_user") {
        return {
          table: "_user",
          col: "id",
        };
      }

      // TODO: We should look up the pk based on `ForeignKey` :/.
    }

    return undefined;
  }

  return (
    <Switch>
      <Match when={getFkTableAndCol()}>
        {(tc) => {
          return (
            <a
              href={encodeURI(
                `/_/admin/table/${tc().table}/?filter=${tc().col}="${props.pk}"`,
              )}
              onClick={(e) => {
                // Prevent sheet from opening.
                e.stopPropagation();
              }}
            >
              {props.children}
            </a>
          );
        }}
      </Match>

      <Match when={true}>{props.children}</Match>
    </Switch>
  );
}
