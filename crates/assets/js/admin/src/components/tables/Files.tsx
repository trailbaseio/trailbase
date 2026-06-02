import { For, Match, Switch, splitProps } from "solid-js";
import { useQuery } from "@tanstack/solid-query";
import { TbOutlineDownload, TbOutlineUpload } from "solid-icons/tb";
import { urlSafeBase64Encode } from "trailbase";

import { sqlValueToString } from "@/lib/value";
import { adminFetch } from "@/lib/fetch";
import { prettyFormatQualifiedName } from "@/lib/schema";
import { showSaveFileDialog } from "@/lib/utils";
import { updateRowInternal } from "@/lib/api/row";

import { Button } from "@/components/ui/button";
import { showToast } from "@/components/ui/toast";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";

import { Column } from "@bindings/Column";
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

type FileUploadInput = {
  name: string | undefined;
  filename: string | undefined;
  content_type: string | undefined;
  data: string;
};

type FileUploadInputs = FileUploadInput[];

async function uploadFiles(opts: {
  tableName: QualifiedName;
  columns: Column[];
  columnName: string;
  pk: {
    columnName: string;
    value: SqlValue;
  };
  multiple: boolean;
  files: FileList;
}) {
  const files = opts.files;
  if (files.length === 0) {
    return;
  }

  if (opts.multiple) {
    const inputs: FileUploadInputs = [];
    for (let i = 0; i < files.length; ++i) {
      inputs.push({
        filename: files[i].name,
        data: urlSafeBase64Encode(new Uint8Array(await files[i].arrayBuffer())),
      } as FileUploadInput);
    }

    const record = {
      [opts.pk.columnName]: opts.pk.value,
      [opts.columnName]: {
        Text: JSON.stringify(inputs),
      },
    };

    await updateRowInternal(opts.tableName, opts.columns, record);
  } else {
    if (files.length > 1) {
      throw new Error("got multiple files");
    }

    const file = files[0];
    const record = {
      [opts.pk.columnName]: opts.pk.value,
      [opts.columnName]: {
        Text: JSON.stringify({
          filename: file.name,
          data: urlSafeBase64Encode(new Uint8Array(await file.arrayBuffer())),
        } as FileUploadInput),
      },
    };

    await updateRowInternal(opts.tableName, opts.columns, record);
  }
}

function FileUploadButton(props: {
  tableName: QualifiedName;
  columns: Column[];
  columnName: string;
  pk: {
    columnName: string;
    value: SqlValue;
  };
  multiple: boolean;
}) {
  let ref: HTMLInputElement | undefined;

  return (
    <div class="pointer-events-none">
      <input
        hidden={true}
        type="file"
        multiple={props.multiple}
        ref={ref}
        onClick={(e) => {
          e.stopPropagation();
        }}
        onChange={(e) => {
          const files = e.target.files;
          if (files !== null && files.length > 0) {
            uploadFiles({
              tableName: props.tableName,
              columns: props.columns,
              columnName: props.columnName,
              multiple: props.multiple,
              pk: props.pk,
              files,
            });
          }
        }}
      />

      <Button
        class="pointer-events-auto"
        variant="outline"
        size="icon"
        onClick={(e) => {
          e.stopPropagation();
          ref?.click();
        }}
      >
        <TbOutlineUpload />
      </Button>
    </div>
  );
}

// TODO: Add a delete button.
function SingleUploadedFile(props: {
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
      <div class="m-1 flex justify-between gap-2 rounded-sm p-1">
        <Tooltip>
          <TooltipTrigger as="div" class="flex flex-col text-left">
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
                <TbOutlineDownload />
              </Button>
            </Match>
          </Switch>
        </div>
      </div>
    </button>
  );
}

export function UploadedFile(props: {
  file: FileUpload | null;
  tableName: QualifiedName;
  columns: Column[];
  columnName: string;
  pk: {
    columnName: string;
    value: SqlValue;
  };
}) {
  return (
    <Switch>
      <Match when={props.file === null}>
        <FileUploadButton
          tableName={props.tableName}
          columns={props.columns}
          columnName={props.columnName}
          pk={props.pk}
          multiple={false}
        />
      </Match>

      <Match when={props.file !== null}>
        <SingleUploadedFile {...props} file={props.file!} />
      </Match>
    </Switch>
  );
}

export function UploadedFiles(props: {
  files: FileUploads;
  tableName: QualifiedName;
  columns: Column[];
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
          return <SingleUploadedFile file={file} {...others} />;
        }}
      </For>

      <FileUploadButton
        tableName={props.tableName}
        columns={props.columns}
        columnName={props.columnName}
        pk={props.pk}
        multiple={true}
      />
    </div>
  );
}

function fileDownloadUrl(opts: {
  tableName: QualifiedName;
  query: ReadFilesQuery;
}): string {
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
