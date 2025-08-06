import { createMemo, createSignal, Switch, Match, Show } from "solid-js";
import { useQuery } from "@tanstack/solid-query";
import { createWritableMemo } from "@solid-primitives/memo";
import { TbDownload, TbColumns, TbColumnsOff } from "solid-icons/tb";

import { adminFetch } from "@/lib/fetch";
import { showSaveFileDialog } from "@/lib/utils";

import { RecordApiConfig } from "@proto/config";
import type { Table } from "@bindings/Table";
import type { TableIndex } from "@bindings/TableIndex";
import type { TableTrigger } from "@bindings/TableTrigger";

import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { iconButtonStyle } from "@/components/IconButton";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";

const modes = ["Insert", "Select", "Update"] as const;
type Mode = (typeof modes)[number];

function SchemaDownloadButton(props: {
  apiName: string;
  mode: Mode;
  schema: object;
}) {
  return (
    <Button
      variant="default"
      onClick={() => {
        // Not supported by firefox:
        // https://developer.mozilla.org/en-US/docs/Web/API/Window/showSaveFilePicker#browser_compatibility
        // possible fallback: https://stackoverflow.com/a/67806663
        showSaveFileDialog({
          contents: JSON.stringify(props.schema, null, "  "),
          filename: `${props.apiName}_${props.mode.toLowerCase()}_schema.json`,
        }).catch(console.error);
      }}
    >
      <TbDownload size={20} />
    </Button>
  );
}

export function SchemaDialog(props: {
  tableName: string;
  apis: RecordApiConfig[];
}) {
  const [mode, setMode] = createSignal<Mode>("Select");
  const apiNames = createMemo(() => props.apis.map((api) => api.name));
  const [apiName, setApiName] = createWritableMemo(() => apiNames()[0] ?? "");

  const schema = useQuery(() => ({
    queryKey: ["schema", mode(), apiName()],
    queryFn: async () => {
      console.debug(`Fetching ${apiName()}: ${mode()}`);
      const response = await adminFetch(
        `/schema/${apiName()}/schema.json?mode=${mode()}`,
      );
      return await response.json();
    },
  }));

  return (
    <Dialog id="schema">
      <DialogTrigger class={iconButtonStyle}>
        <Tooltip>
          <TooltipTrigger as="div">
            <TbColumns size={20} />
          </TooltipTrigger>

          <TooltipContent>JSON Schemas of "{props.tableName}"</TooltipContent>
        </Tooltip>
      </DialogTrigger>

      <DialogContent class="min-w-[80dvw]">
        <DialogHeader>
          <div class="mr-4 flex items-center justify-between">
            <DialogTitle>JSON Schema "{apiName()}"</DialogTitle>

            <div class="flex items-center gap-2">
              <Show when={schema.isSuccess}>
                <SchemaDownloadButton
                  apiName={apiName()}
                  schema={schema.data}
                  mode={mode()}
                />
              </Show>

              {/* NOTE: This is for tables with multiple APIs */}
              {apiNames().length > 1 && (
                <Select
                  value={apiName()}
                  onChange={setApiName}
                  options={[...apiNames()]}
                  placeholder="API"
                  itemComponent={(props) => (
                    <SelectItem item={props.item}>
                      {props.item.rawValue}
                    </SelectItem>
                  )}
                >
                  <SelectTrigger aria-label="Apis" class="w-[180px]">
                    <SelectValue>
                      {(state) => `API: ${state.selectedOption()}`}
                    </SelectValue>
                  </SelectTrigger>
                  <SelectContent />
                </Select>
              )}

              <Select
                value={mode()}
                onChange={setMode}
                options={[...modes]}
                placeholder="Mode"
                itemComponent={(props) => (
                  <SelectItem item={props.item}>
                    {props.item.rawValue}
                  </SelectItem>
                )}
              >
                <SelectTrigger aria-label="Mode" class="w-[180px]">
                  <SelectValue>
                    {(state) => `Mode: ${state.selectedOption()}`}
                  </SelectValue>
                </SelectTrigger>
                <SelectContent />
              </Select>
            </div>
          </div>
        </DialogHeader>

        <div class="h-[80dvh] overflow-auto">
          <Switch>
            <Match when={schema.error}>{`Error: ${schema.error}`}</Match>

            <Match when={schema.isLoading}>Loading...</Match>

            <Match when={schema.data}>
              <pre>{JSON.stringify(schema.data!, null, "  ")}</pre>
            </Match>
          </Switch>
        </div>
      </DialogContent>
    </Dialog>
  );
}

export function DebugSchemaDialogButton(props: {
  table: Table;
  indexes: TableIndex[];
  triggers: TableTrigger[];
}) {
  const columns = () => props.table.columns;
  const indexes = () => props.indexes;
  const triggers = () => props.triggers;
  const fks = () => props.table.foreign_keys;

  return (
    <Dialog id="schema">
      <DialogTrigger class={iconButtonStyle}>
        <TbColumnsOff size={20} />
      </DialogTrigger>

      <DialogContent class="min-w-[80dvw]">
        <DialogHeader>
          <DialogTitle>Schema</DialogTitle>
        </DialogHeader>

        <div class="max-h-[80dvh] overflow-auto">
          <div class="mx-2 flex flex-col gap-2">
            <h3>Columns</h3>
            <pre class="w-[70vw] overflow-x-hidden text-xs">
              {JSON.stringify(columns(), null, 2)}
            </pre>

            <h3>Foreign Keys</h3>
            <pre class="w-[70vw] overflow-x-hidden text-xs">
              {JSON.stringify(fks(), null, 2)}
            </pre>

            <h3>Indexes</h3>
            <pre class="w-[70vw] overflow-x-hidden text-xs">
              {JSON.stringify(indexes(), null, 2)}
            </pre>

            <h3>Triggers</h3>
            <pre class="w-[70vw] overflow-x-hidden text-xs">
              {JSON.stringify(triggers(), null, 2)}
            </pre>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
