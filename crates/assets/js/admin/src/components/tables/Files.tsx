import { For, Match, Switch, createMemo } from "solid-js";
import { useQuery } from "@tanstack/solid-query";

import { sqlValueToString } from "@/lib/value";
import { adminFetch } from "@/lib/fetch";
import { prettyFormatQualifiedName } from "@/lib/schema";

import type { QualifiedName } from "@bindings/QualifiedName";
import type { ReadFilesQuery } from "@bindings/ReadFilesQuery";
import type { SqlValue } from "@bindings/SqlValue";

export type FileUpload = {
  id: string;
  original_filename: string | undefined;
  filename: string | undefined;
  content_type: string | undefined;
  mime_type: string | undefined;
};

export type FileUploads = FileUpload[];

export function UploadedFile(props: {
  file: FileUpload;
  tableName: QualifiedName;
  columnName: string;
  pk: {
    columnName: string;
    value: SqlValue;
  };
}) {
  const isImage = () =>
    isImageMime(props.file.mime_type ?? props.file.content_type);
  const url = () =>
    fileDownloadUrl({
      tableName: props.tableName,
      query: {
        pk_column: props.pk.columnName,
        pk_value: sqlValueToString(props.pk.value),
        file_column_name: props.columnName,
        file_name: props.file.filename ?? null,
      },
    });

  return (
    <Switch>
      <Match when={isImage()}>
        <Image url={url()} mime={props.file.mime_type} />
      </Match>

      <Match when={!isImage()}>{JSON.stringify(props.file)}</Match>
    </Switch>
  );
}

export function UploadedFiles(props: {
  files: FileUploads;
  tableName: QualifiedName;
  columnName: string;
  pk: {
    columnName: string;
    value: SqlValue;
  };
}) {
  const indexes = createMemo(() => {
    const indexes: number[] = [];
    for (let i = 0; i < props.files.length; ++i) {
      const file = props.files[i];
      if (isImageMime(file.mime_type ?? file.content_type)) {
        indexes.push(i);
      }

      if (indexes.length >= 3) break;
    }

    return indexes;
  });

  return (
    <Switch>
      <Match when={indexes().length > 0}>
        <div class="flex gap-2">
          <For each={indexes()}>
            {(index: number) => {
              const fileUpload = props.files[index];
              const url = fileDownloadUrl({
                tableName: props.tableName,
                query: {
                  pk_column: props.pk.columnName,
                  pk_value: sqlValueToString(props.pk.value),
                  file_column_name: props.columnName,
                  file_name: fileUpload.filename ?? null,
                },
              });

              return <Image url={url} mime={fileUpload.mime_type} />;
            }}
          </For>
        </div>
      </Match>

      <Match when={indexes().length === 0}>{JSON.stringify(props.files)}</Match>
    </Switch>
  );
}

function fileDownloadUrl(opts: {
  tableName: QualifiedName;
  query: ReadFilesQuery;
}): string {
  // const origin = import.meta.env.DEV
  //   ? "http://localhost:4000"
  //   : window.location.origin;
  const tableName: string = prettyFormatQualifiedName(opts.tableName);
  const query = opts.query;

  if (query.file_name) {
    const params = new URLSearchParams({
      pk_column: query.pk_column,
      pk_value: query.pk_value,
      file_column_name: query.file_column_name,
      file_name: query.file_name,
    });

    // return `${origin}/api/_admin/table/${tableName}/files?${params}`;
    return `/table/${tableName}/files?${params}`;
  }

  const params = new URLSearchParams({
    pk_column: query.pk_column,
    pk_value: query.pk_value,
    file_column_name: query.file_column_name,
  });
  return `/table/${tableName}/files?${params}`;
}

function Image(props: { url: string; mime: string | undefined }) {
  const imageData = useQuery(() => ({
    queryKey: ["tableImage", props.url],
    queryFn: async () => {
      const response = await adminFetch(props.url);
      return await asyncBase64Encode(await response.blob());
    },
  }));

  return (
    <Switch>
      <Match when={imageData.isError}>{`${imageData.error}`}</Match>

      <Match when={imageData.isLoading}>Loading</Match>

      <Match when={imageData.data}>
        <img class="size-[50px]" src={imageData.data} />
      </Match>
    </Switch>
  );
}

function isImageMime(mime: string | undefined): boolean {
  switch (mime) {
    case "image/jpeg":
      return true;
    case "image/png":
      return true;
    default:
      return false;
  }
}

function asyncBase64Encode(blob: Blob): Promise<string> {
  return new Promise((resolve, _) => {
    const reader = new FileReader();
    reader.onloadend = () => resolve(reader.result as string);
    reader.readAsDataURL(blob);
  });
}
