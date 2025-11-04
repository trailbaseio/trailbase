import { For, Match, Switch, splitProps } from "solid-js";
import { useQuery } from "@tanstack/solid-query";
import { TbDownload } from "solid-icons/tb";

import { sqlValueToString } from "@/lib/value";
import { adminFetch } from "@/lib/fetch";
import { prettyFormatQualifiedName } from "@/lib/schema";
import { showSaveFileDialog } from "@/lib/utils";

import { Button } from "@/components/ui/button";
import { showToast } from "@/components/ui/toast";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";

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
    <button
      onClick={(event) => {
        // Prevent edit record form from opening.
        event.stopPropagation();

        // Open download dialog.
        (async () => {
          const success = await showSaveFileDialog({
            filename:
              props.file.filename ?? props.file.original_filename ?? "download",
            mimeType: props.file.mime_type ?? props.file.content_type,
            contents: async () => {
              const response = await adminFetch(url());
              return response.body;
            },
          });

          if (success) {
            showToast({
              title: `Downloaded: ${props.file.filename}`,
              variant: "success",
            });
          }
        })();
      }}
    >
      <div class="m-1 flex justify-between gap-2 rounded p-1">
        <Tooltip>
          <TooltipTrigger as="div" class="flex flex-col items-start">
            <p>{props.file.original_filename ?? props.file.filename}</p>
            <p>mime: {contentType(props.file)}</p>
          </TooltipTrigger>

          <TooltipContent>
            <p>id: {props.file.id}</p>
            <p>filename: {props.file.filename}</p>
            <p>original: {props.file.original_filename}</p>
            <p>content: {props.file.content_type}</p>
            <p>mime: {props.file.mime_type}</p>
          </TooltipContent>
        </Tooltip>

        <div class="content-center">
          <Switch>
            <Match when={isImage()}>
              <Image url={url()} mime={props.file.mime_type} />
            </Match>

            <Match when={!isImage()}>
              <Button as="div" size="icon" variant="outline">
                <TbDownload />
              </Button>
            </Match>
          </Switch>
        </div>
      </div>
    </button>
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
  const [local, others] = splitProps(props, ["files"]);

  return (
    <div class="flex flex-col gap-2">
      <For each={local.files}>
        {(file: FileUpload) => {
          return <UploadedFile file={file} {...others} />;
        }}
      </For>
    </div>
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
    <div class="size-[50px]">
      <Switch>
        <Match when={imageData.isError}>{`${imageData.error}`}</Match>

        <Match when={imageData.isLoading}>Loading</Match>

        <Match when={imageData.data}>
          <img src={imageData.data} />
        </Match>
      </Switch>
    </div>
  );
}

function contentType(file: FileUpload): string {
  const mimeType = file.mime_type ?? file.content_type;
  if (mimeType === undefined) {
    return "?";
  }

  const components = mimeType.split("/");
  return components.at(-1) ?? "?";
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
