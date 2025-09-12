import {
  For,
  Match,
  Show,
  Switch,
  createEffect,
  createSignal,
  onCleanup,
  type Accessor,
  type Signal,
} from "solid-js";
import { useQuery } from "@tanstack/solid-query";
import { createWritableMemo } from "@solid-primitives/memo";
import type { ColumnDef } from "@tanstack/solid-table";
import { persistentAtom } from "@nanostores/persistent";
import { useStore } from "@nanostores/solid";
import {
  TbTrash,
  TbEdit,
  TbDeviceFloppy,
  TbHelp,
  TbPencilPlus,
} from "solid-icons/tb";

import { autocompletion } from "@codemirror/autocomplete";
import { EditorView, lineNumbers, keymap } from "@codemirror/view";
import { EditorState } from "@codemirror/state";
import { minimalSetup } from "codemirror";
import { sql, SQLConfig, SQLNamespace, SQLite } from "@codemirror/lang-sql";

import { iconButtonStyle, IconButton } from "@/components/IconButton";
import { Header } from "@/components/Header";
import { SplitView } from "@/components/SplitView";
import {
  Resizable,
  ResizablePanel,
  ResizableHandle,
} from "@/components/ui/resizable";
import { Callout } from "@/components/ui/callout";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
  DialogFooter,
} from "@/components/ui/dialog";
import { TextField, TextFieldInput } from "@/components/ui/text-field";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { showToast } from "@/components/ui/toast";
import { DataTable } from "@/components/Table";

import { createTableSchemaQuery } from "@/lib/table";
import type { QueryResponse } from "@bindings/QueryResponse";
import type { ListSchemasResponse } from "@bindings/ListSchemasResponse";
import { executeSql, type ExecutionResult } from "@/lib/fetch";
import { isNotNull } from "@/lib/schema";

type RowData = Array<object>;

function buildSchema(schemas: ListSchemasResponse): SQLNamespace {
  const schema: {
    [name: string]: SQLNamespace;
  } = {};

  for (const table of schemas.tables) {
    const tableName = table.name.name;
    schema[tableName] = {
      self: { label: tableName, type: "keyword" },
      children: table.columns.map((c) => c.name),
    } satisfies SQLNamespace;
  }

  for (const view of schemas.views) {
    const viewName = view.name.name;
    schema[viewName] = {
      self: { label: viewName, type: "keyword" },
      children: view.column_mapping?.columns.map((c) => c.column.name) ?? [],
    } satisfies SQLNamespace;
  }

  return schema;
}

function ConfirmSwitchDialog(props: {
  back: () => void;
  confirm: () => void;
  saveScript: () => void;
  message?: string;
}) {
  return (
    <DialogContent>
      <DialogTitle>Confirmation</DialogTitle>

      <p>{props.message ?? "Are you sure?"}</p>

      <DialogFooter>
        <div class="flex w-full justify-between">
          <Button variant="outline" onClick={props.back}>
            Back
          </Button>

          <div class="flex gap-4">
            <Button variant="destructive" onClick={props.confirm}>
              Discard
            </Button>

            <Button
              variant="default"
              onClick={() => {
                props.saveScript();
                props.confirm();
              }}
            >
              Save
            </Button>
          </div>
        </div>
      </DialogFooter>
    </DialogContent>
  );
}

function ResultView(props: {
  script: Script;
  response: ExecutionResult | undefined;
}) {
  const response = () => props.response ?? props.script.result;

  function columnDefs(data: QueryResponse): ColumnDef<RowData>[] {
    return (data.columns ?? []).map((col, idx) => {
      const notNull = isNotNull(col.options);

      const header = `${col.name} [${col.data_type}${notNull ? "" : "?"}]`;
      return {
        accessorFn: (row) => row[idx],
        header,
      };
    });
  }

  return (
    <Show when={response()} fallback={<>No Data</>}>
      <Switch>
        <Match when={response()?.error}>
          Error: {response()?.error?.message}
        </Match>

        <Match when={(response()?.data?.columns?.length ?? 0) > 0}>
          <div class="flex flex-col gap-2">
            <div class="flex justify-end text-sm">
              Last executed:{" "}
              {new Date(response()?.timestamp ?? 0).toLocaleTimeString()}
            </div>

            {/* TODO: Enable pagination */}
            <DataTable
              columns={() => columnDefs(response()!.data!)}
              data={() => response()!.data!.rows as RowData[]}
              pagination={{
                pageIndex: 0,
                pageSize: 50,
              }}
            />
          </div>
        </Match>

        <Match when={(response()?.data?.columns?.length ?? 0) == 0}>
          No data returned by query
        </Match>
      </Switch>
    </Show>
  );
}

function SideBar(props: {
  selected: number;
  setSelected: (idx: number) => void;
  dirty: boolean;
  horizontal: boolean;
}) {
  const scripts = useStore($scripts);

  const addNewScript = () => props.setSelected(createNewScript());

  const flexStyle = () => (props.horizontal ? "flex flex-col h-dvh" : "flex");
  return (
    <div class={`${flexStyle()} m-4 gap-2`}>
      <Button class="flex gap-2" variant="secondary" onClick={addNewScript}>
        <TbPencilPlus size={20} /> New
      </Button>

      <For each={scripts()}>
        {(_script: Script, i: Accessor<number>) => {
          const scriptName = () => scripts()[i()].name;
          return (
            <Button
              variant={props.selected === i() ? "default" : "outline"}
              onClick={() => props.setSelected(i())}
            >
              {props.selected === i() && props.dirty
                ? `${scriptName()}*`
                : scriptName()}
            </Button>
          );
        }}
      </For>
    </div>
  );
}

function HelpDialog() {
  return (
    <Dialog id="edit-help">
      <DialogTrigger class={iconButtonStyle}>
        <TbHelp size={20} />
      </DialogTrigger>

      <DialogContent>
        <DialogHeader>
          <DialogTitle>Editor Help</DialogTitle>
        </DialogHeader>

        <p>
          The editor lets you execute arbitrary SQL statements, so be careful
          with what you wish for. If you just want to experiment, consider
          working on a non-prod data set or a copy.
        </p>

        <p>
          Further note that there's no pagination, so whatever you query will be
          returned. Working on large data sets, you might want to{" "}
          <span class="font-mono">LIMIT</span> your result size.
        </p>

        <p>
          Also note that scripts are currently stored in your browser's local
          storage. This means, switching devices, browsers, or the origin of
          your website, you won't have access to your scripts. This is something
          we'd like to lower into the database layer in the future.
        </p>
      </DialogContent>
    </Dialog>
  );
}

function RenameDialog(props: { selected: number; script: Script }) {
  const [open, setOpen] = createSignal(false);
  const [name, setName] = createWritableMemo(() => props.script.name);

  return (
    <Dialog id="rename" open={open()} onOpenChange={setOpen}>
      <DialogTrigger class={iconButtonStyle}>
        <Tooltip>
          <TooltipTrigger as="div">
            <TbEdit size={20} />
          </TooltipTrigger>
          <TooltipContent>Rename script</TooltipContent>
        </Tooltip>
      </DialogTrigger>

      <DialogContent>
        <DialogHeader>
          <DialogTitle>Rename</DialogTitle>
        </DialogHeader>

        <form
          class="flex flex-col gap-4"
          method="dialog"
          onSubmit={(e: SubmitEvent) => {
            e.preventDefault();

            updateExistingScript(props.selected, {
              ...props.script,
              name: name(),
            });
            setOpen(false);
          }}
        >
          <TextField>
            <TextFieldInput
              required
              value={name()}
              type="text"
              onChange={(e: Event) => {
                setName((e.target as HTMLInputElement).value);
              }}
            />
          </TextField>

          <DialogFooter>
            <Button type="submit">Save</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

type DirtyDialogState = {
  nextSelected: number;
};

function EditorPanel(props: {
  schemas: ListSchemasResponse;
  script: Script;
  selected: Signal<number>;
  dirty: Signal<boolean>;
  dirtyDialog: Signal<DirtyDialogState | undefined>;
  deleteScript: () => void;
}) {
  // eslint-disable-next-line solid/reactivity
  const [dirty, setDirty] = props.dirty;
  // eslint-disable-next-line solid/reactivity
  const [dialog, setDialog] = props.dirtyDialog;
  // eslint-disable-next-line solid/reactivity
  const [selected, setSelected] = props.selected;

  const [showCallout, setShowCallout] = createSignal(true);

  // Will only be set when the user explicitly triggers "execute";
  const [queryString, setQueryString] = createWritableMemo<string | null>(
    () => {
      // Reset whenever we switch tabs.
      const _unused = selected();

      return null;
    },
  );

  const executionResult = useQuery(() => {
    const query = queryString();
    const queryKey = query ?? props.script.contents;
    return {
      queryKey: ["query", queryKey, selected()],
      queryFn: async () => {
        if (query === null) {
          return null;
        }

        const response = await executeSql(query);
        const error = response.error;
        if (error) {
          showToast({
            title: "Execution Error",
            description: error.message,
            variant: "error",
          });
        }

        // Update the scripts state.
        updateExistingScript(selected(), {
          ...props.script,
          result: response,
        });

        return response;
      },
    };
  });

  const execute = () => {
    const text = editor?.state.doc.toString();
    if (text) {
      // We need to distinguish new query vs same query to make sure we're not caching.
      if (queryString() === text) {
        executionResult.refetch();
      } else {
        setQueryString(text);
      }
    }
  };

  const saveScript = () => {
    if (editor) {
      updateExistingScript(selected(), {
        ...props.script,
        contents: editor.state.doc.toString(),
      });
    }
    setDirty(false);
  };

  let ref: HTMLDivElement | undefined;
  let editor: EditorView | undefined;

  const customKeymap = keymap.of([
    {
      key: "Ctrl-Enter",
      run: () => {
        execute();
        return true;
      },
      preventDefault: true,
    },
  ]);

  const newEditorState = (contents: string) => {
    return EditorState.create({
      doc: contents,
      extensions: [
        myTheme,
        customKeymap,
        lineNumbers(),
        // Let's you define your own custom CSS style for the line number gutter.
        // gutter({ class: "cm-mygutter" }),
        sql({
          dialect: SQLite,
          upperCaseKeywords: true,
          schema: buildSchema(props.schemas),
        } as SQLConfig),
        autocompletion(),
        EditorView.updateListener.of((v) => {
          if (!v.changes.empty) {
            setDirty(true);
          }
        }),
        // NOTE: minimal setup provides a bunch of default extensions such as
        // keymaps, undo history, default syntax highlighting ... .
        // NOTE: should be last.
        minimalSetup,
      ],
    });
  };

  onCleanup(() => editor?.destroy());

  createEffect(() => {
    // Every time the script contents change, recreate the editor state.
    editor?.destroy();
    editor = new EditorView({
      state: newEditorState(props.script.contents),
      parent: ref!,
    });
    editor.focus();
  });

  const LeftButtons = () => (
    <>
      <RenameDialog selected={selected()} script={props.script} />

      <IconButton tooltip="Save script" onClick={saveScript}>
        <TbDeviceFloppy size={20} />
      </IconButton>

      <IconButton tooltip="Delete this script" onClick={props.deleteScript}>
        <TbTrash size={20} />
      </IconButton>
    </>
  );

  return (
    <Dialog
      id="switch-script-dialog"
      open={dialog() !== undefined}
      onOpenChange={(isOpen) => {
        if (!isOpen) {
          setDialog();
        }
      }}
      modal={true}
    >
      <ConfirmSwitchDialog
        back={() => setDialog()}
        confirm={() => {
          const state = dialog();
          if (state) {
            setSelected(state.nextSelected);
            setDialog();
            setDirty(false);
          }
        }}
        saveScript={saveScript}
        message="Proceeding will discard any pending changes in the current buffer. Proceed with caution."
      />

      <Resizable orientation="vertical" class="overflow-hidden">
        <ResizablePanel class="flex flex-col">
          <Header
            title="Editor"
            titleSelect={dirty() ? `${props.script.name}*` : props.script.name}
            left={<LeftButtons />}
            right={<HelpDialog />}
          />

          <div class="mx-4 my-2 flex grow flex-col gap-2">
            {showCallout() && (
              <Callout
                class="text-sm hover:opacity-[80%]"
                onClick={() => setShowCallout(false)}
              >
                When changing schemas, consider using migrations to consistently
                apply changes across environments. One-off alterations can
                otherwise lead to skew. Alterations using the table browser will
                produce migrations.
              </Callout>
            )}

            {/* Editor */}
            <div
              class="max-h-[70dvh] grow overflow-y-scroll rounded outline"
              ref={ref}
            />

            <div class="flex justify-end">
              <Tooltip>
                <TooltipTrigger as="div">
                  <Button variant="destructive" onClick={execute}>
                    Execute (Ctrl+Enter)
                  </Button>
                </TooltipTrigger>
                <TooltipContent>
                  Execute script on the server. No turning back.
                </TooltipContent>
              </Tooltip>
            </div>
          </div>
        </ResizablePanel>

        <ResizableHandle withHandle={true} />

        <ResizablePanel class="hide-scrollbars overflow-y-scroll">
          <div class="grow p-4">
            <ResultView
              script={props.script}
              response={executionResult.data ?? undefined}
            />
          </div>
        </ResizablePanel>
      </Resizable>
    </Dialog>
  );
}

export function EditorPage() {
  const scripts = useStore($scripts);
  const [selected, setSelected] = createSignal<number>(0);
  const [dirty, setDirty] = createSignal<boolean>(false);

  const schemaFetch = createTableSchemaQuery();

  const [dialog, setDialog] = createSignal<DirtyDialogState | undefined>();
  const switchToScript = (idx: number) => {
    if (dirty()) {
      setDialog({ nextSelected: idx });
    } else {
      setSelected(idx);
    }
  };

  const script = (idx?: number): Script => {
    const s = scripts();
    const i = idx ?? selected();
    if (i < s.length) {
      return s[i];
    }
    if (s.length === 0) {
      return defaultScript;
    }
    return s[s.length - 1];
  };

  const deleteCurrentScript = () => {
    const idx = selected();
    deleteScript(idx);
    setSelected(Math.max(0, idx - 1));
  };

  return (
    <SplitView
      first={(props: { horizontal: boolean }) => {
        return (
          <SideBar
            selected={selected()}
            setSelected={switchToScript}
            dirty={dirty()}
            horizontal={props.horizontal}
          />
        );
      }}
      second={() => {
        return (
          <Switch fallback={"Loading..."}>
            <Match when={schemaFetch.isError}>
              <span>
                Schema fetch error: {JSON.stringify(schemaFetch.error)}
              </span>
            </Match>

            <Match when={schemaFetch.data}>
              <EditorPanel
                schemas={schemaFetch.data!}
                selected={[selected, setSelected]}
                script={script()}
                dirty={[dirty, setDirty]}
                dirtyDialog={[dialog, setDialog]}
                deleteScript={deleteCurrentScript}
              />
            </Match>
          </Switch>
        );
      }}
    />
  );
}

export default EditorPage;

const myTheme = EditorView.theme(
  {
    ".cm-gutters": {
      backgroundColor: "#eeeeee",
      color: "#000",
      border: "none",
    },
  },
  { dark: false },
);

type Script = {
  name: string;
  contents: string;

  result?: ExecutionResult;
};

const defaultScript: Script = {
  name: "Select Users",
  contents: "SELECT\n  *\nFROM\n  _user;",
};

// NOTE: It seems like "nanostores" diffs array contents. It re-renders, if the array
// object is different and at least one of the contained objects has a different id.
// In other words just copying the array and setting a new Script.name, doesn't trigger,
// we have to replace the entire script.
// If this behavior is documented somewhere, I couldn't find it. I wish it would be less
// smart :/.
function updateExistingScript(index: number, script: Script) {
  const s = [...$scripts.get()];
  s[index] = {
    ...script,
  };
  $scripts.set(s);
}

function createNewScript(): number {
  const s = [
    ...$scripts.get(),
    {
      name: "New Script",
      contents: defaultScript.contents,
    },
  ];
  $scripts.set(s);
  return s.length - 1;
}

function deleteScript(idx: number) {
  $scripts.set($scripts.get().toSpliced(idx, 1));
}

const $scripts = persistentAtom<Script[]>("scripts", [defaultScript], {
  encode: JSON.stringify,
  decode: JSON.parse,
});
