import { createSignal, Switch, Match, Show } from "solid-js";
import { useQuery } from "@tanstack/solid-query";
import { TbDownload, TbBug } from "solid-icons/tb";

import { adminFetch } from "@/lib/fetch";
import { showSaveFileDialog, stringToReadableStream } from "@/lib/utils";

import { RecordApiConfig } from "@proto/config";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardTitle, CardHeader } from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { IconButton } from "@/components/IconButton";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

const modes = ["Create", "Read", "Update"] as const;
type Mode = (typeof modes)[number];

function mapMode(mode: Mode): string {
  switch (mode) {
    case "Create":
      return "Insert";
    case "Read":
      return "Select";
    case "Update":
      return "Update";
  }
}

function SchemaDownloadButton(props: {
  apiName: string;
  mode: Mode;
  schema: string;
}) {
  return (
    <Button
      variant="outline"
      onClick={() => {
        showSaveFileDialog({
          contents: async () =>
            stringToReadableStream(JSON.stringify(props.schema, null, "  ")),
          filename: `${props.apiName}_${props.mode.toLowerCase()}_schema.json`,
        });
      }}
    >
      Download
      <TbDownload />
    </Button>
  );
}

export function SchemaCard(props: { api: RecordApiConfig }) {
  const [mode, setMode] = createSignal<Mode>("Create");
  const apiName = () => props.api.name ?? "??";

  const schema = useQuery(() => ({
    queryKey: ["schema", mode(), apiName()],
    queryFn: async () => {
      console.debug(`Fetching ${apiName()}: ${mode()}`);
      const response = await adminFetch(
        `/schema/${apiName()}/schema.json?mode=${mapMode(mode())}`,
      );
      return await response.json();
    },
  }));

  return (
    <Card>
      <CardHeader>
        <CardTitle>JSON Schema</CardTitle>
      </CardHeader>

      <CardContent class="flex flex-col gap-4">
        <div class="flex w-full items-center justify-between gap-2">
          <Select
            value={mode()}
            onChange={setMode}
            options={[...modes]}
            placeholder="Mode"
            itemComponent={(props) => (
              <SelectItem item={props.item}>{props.item.rawValue}</SelectItem>
            )}
          >
            <SelectTrigger aria-label="Mode" class="w-[180px]">
              <SelectValue>
                {(state) => `${state.selectedOption()}`}
              </SelectValue>
            </SelectTrigger>
            <SelectContent />
          </Select>

          <Show when={schema.isSuccess}>
            <SchemaDownloadButton
              apiName={apiName()}
              schema={schema.data}
              mode={mode()}
            />
          </Show>
        </div>

        <Switch>
          <Match when={schema.isError}>{`Error: ${schema.error}`}</Match>

          <Match when={schema.isLoading}>Loading...</Match>

          <Match when={schema.isSuccess}>
            <span class="overflow-x-scroll font-mono text-sm whitespace-pre-wrap">
              {JSON.stringify(schema.data, null, "  ")}
            </span>
          </Match>
        </Switch>
      </CardContent>
    </Card>
  );
}

export function DebugDialogButton(props: { title: string; data: object }) {
  return (
    <Dialog id="schema">
      <DialogTrigger>
        <IconButton tooltip="[DEV only]">
          <TbBug />
        </IconButton>
      </DialogTrigger>

      <DialogContent class="max-w-[80dvw]">
        <DialogHeader>
          <DialogTitle>[Debug] {props.title}</DialogTitle>
        </DialogHeader>

        <div class="max-h-[80dvh] overflow-auto">
          <div class="mx-2 flex flex-col gap-2">
            <h3>{props.title}</h3>

            <pre class="w-[70vw] overflow-x-hidden text-xs">
              {JSON.stringify(props.data, null, 2)}
            </pre>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
