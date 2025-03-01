import { createSignal, createResource, Switch, Match, Show } from "solid-js";
import { adminFetch } from "@/lib/fetch";
import { TbDownload, TbColumns, TbColumnsOff } from "solid-icons/tb";
import { showSaveFileDialog } from "@/lib/utils";
import { iconButtonStyle } from "@/components/IconButton";

import type { Table, TableIndex, TableTrigger } from "@/lib/bindings";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
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
  tableName: string;
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
          filename: `${props.tableName}_${props.mode.toLowerCase()}_schema.json`,
        }).catch(console.error);
      }}
    >
      <TbDownload size={20} />
    </Button>
  );
}

export function SchemaDialog(props: { tableName: string }) {
  const [mode, setMode] = createSignal<Mode>("Select");
  const [schema] = createResource(mode, async (mode) => {
    console.debug(`Fetching ${props.tableName}: ${mode}`);
    const response = await adminFetch(
      `/table/${props.tableName}/schema.json?mode=${mode}`,
    );
    return await response.json();
  });

  return (
    <Dialog id="schema">
      <DialogTrigger class={iconButtonStyle}>
        <Tooltip>
          <TooltipTrigger as="div">
            <TbColumns size={20} />
          </TooltipTrigger>
          <TooltipContent>JSON Schema of "{props.tableName}"</TooltipContent>
        </Tooltip>
      </DialogTrigger>

      <DialogContent class="min-w-[80dvw]">
        <DialogHeader>
          <div class="mr-4 flex items-center justify-between">
            <DialogTitle>JSON Schema</DialogTitle>

            <div class="flex items-center gap-2">
              <Show when={schema.state === "ready"}>
                <SchemaDownloadButton
                  tableName={props.tableName}
                  schema={schema()}
                  mode={mode()}
                />
              </Show>

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
                <SelectTrigger aria-label="Fruit" class="w-[180px]">
                  <SelectValue<string>>
                    {(state) => state.selectedOption()}
                  </SelectValue>
                </SelectTrigger>
                <SelectContent />
              </Select>
            </div>
          </div>
        </DialogHeader>

        <div class="h-[80dvh] overflow-auto">
          <Switch>
            <Match when={schema.error}>Error: {schema.error}</Match>

            <Match when={schema.loading}>Loading...</Match>

            <Match when={schema()}>
              <pre>{JSON.stringify(schema(), null, "  ")}</pre>
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
