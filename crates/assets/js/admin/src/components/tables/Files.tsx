import { For, Match, Switch, splitProps } from "solid-js";
import { useQuery } from "@tanstack/solid-query";
import {
  TbOutlineDownload,
  TbOutlineUpload,
  TbOutlineTrash,
} from "solid-icons/tb";
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
  data: string | undefined;
};

async function uploadSingleFile(opts: {
  tableName: QualifiedName;
  columns: Column[];
  columnName: string;
  pk: {
    columnName: string;
    value: SqlValue;
  };
  file: File;
}) {
  const record = {
    [opts.pk.columnName]: opts.pk.value,
    [opts.columnName]: {
      Text: JSON.stringify({
        filename: opts.file.name,
        data: urlSafeBase64Encode(
          new Uint8Array(await opts.file.arrayBuffer()),
        ),
      } as FileUploadInput),
    },
  };

  await updateRowInternal(opts.tableName, opts.columns, record);
}

async function uploadMultipleFiles(opts: {
  tableName: QualifiedName;
  columns: Column[];
  columnName: string;
  pk: {
    columnName: string;
    value: SqlValue;
  };
  files: FileList;
  allFiles: FileUploads;
}) {
  const files = opts.files;
  if (files.length === 0) {
    return;
  }

  const inputs: (FileUploadInput | FileUpload)[] = [...opts.allFiles];
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
}

async function deleteSingleFile(opts: {
  tableName: QualifiedName;
  columns: Column[];
  columnName: string;
  pk: {
    columnName: string;
    value: SqlValue;
  };
  multiple: boolean;
  file: FileUpload;
  allFiles: FileUploads;
}) {
  if (opts.multiple) {
    // NOTE: We don't currently have the ability to delete individual files.
    // Instead we override with the ones to keep.
    const keep = opts.allFiles.filter((f) => f.filename !== opts.file.filename);

    const record = {
      [opts.pk.columnName]: opts.pk.value,
      [opts.columnName]: {
        Text: JSON.stringify(keep),
      },
    };

    await updateRowInternal(opts.tableName, opts.columns, record);
  } else {
    const record = {
      [opts.pk.columnName]: opts.pk.value,
      [opts.columnName]: "Null" as SqlValue,
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
  allFiles: FileUploads;
  rowsRefetch: () => void;
}) {
  let ref: HTMLInputElement | undefined;

  const uploadFiles = async (files: FileList) => {
    if (files.length === 0) {
      return;
    }

    if (props.multiple) {
      await uploadMultipleFiles({
        tableName: props.tableName,
        columns: props.columns,
        columnName: props.columnName,
        pk: props.pk,
        files,
        allFiles: props.allFiles,
      });
    } else {
      if (files.length > 1) {
        throw new Error("got multiple files");
      }

      await uploadSingleFile({
        tableName: props.tableName,
        columns: props.columns,
        columnName: props.columnName,
        pk: props.pk,
        file: files[0],
      });
    }

    showToast({
      title: `Uploaded ${files.length} files`,
      variant: "success",
    });
    props.rowsRefetch();
  };

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
          if (files === null) {
            return;
          }

          uploadFiles(files);
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

function SingleUploadedFile(props: {
  file: FileUpload;
  allFiles: FileUploads;
  tableName: QualifiedName;
  columns: Column[];
  columnName: string;
  pk: {
    columnName: string;
    value: SqlValue;
  };
  multiple: boolean;
  rowsRefetch: () => void;
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
  const deleteFile = async () => {
    await deleteSingleFile(props);

    showToast({
      title: `Deleted: ${props.file.filename}`,
      variant: "success",
    });
    props.rowsRefetch();
  };

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

          <Button
            as="div"
            size="icon"
            variant="outline"
            onClick={(e) => {
              e.stopPropagation();
              deleteFile();
            }}
          >
            <TbOutlineTrash />
          </Button>
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
  rowsRefetch: () => void;
}) {
  return (
    <Switch>
      <Match when={props.file === null}>
        <FileUploadButton
          tableName={props.tableName}
          columns={props.columns}
          columnName={props.columnName}
          pk={props.pk}
          allFiles={[props.file!]}
          multiple={false}
          rowsRefetch={props.rowsRefetch}
        />
      </Match>

      <Match when={props.file !== null}>
        <SingleUploadedFile
          {...props}
          multiple={false}
          file={props.file!}
          allFiles={[props.file!]}
        />
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
  rowsRefetch: () => void;
}) {
  const [local, others] = splitProps(props, ["files"]);

  return (
    <div class="flex flex-col gap-2">
      <For each={local.files}>
        {(file: FileUpload) => {
          return (
            <SingleUploadedFile
              multiple={true}
              {...others}
              file={file}
              allFiles={props.files}
            />
          );
        }}
      </For>

      <FileUploadButton
        tableName={props.tableName}
        columns={props.columns}
        columnName={props.columnName}
        pk={props.pk}
        allFiles={props.files}
        multiple={true}
        rowsRefetch={props.rowsRefetch}
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
