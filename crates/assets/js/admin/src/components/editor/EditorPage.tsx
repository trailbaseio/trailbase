import {
  ErrorBoundary,
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
import { TbTrash, TbEdit, TbHelp, TbPencilPlus, TbX } from "solid-icons/tb";

import { autocompletion } from "@codemirror/autocomplete";
import { EditorView, lineNumbers, keymap } from "@codemirror/view";
import { EditorState } from "@codemirror/state";
import { minimalSetup } from "codemirror";
import { sql, SQLConfig, SQLNamespace, SQLite } from "@codemirror/lang-sql";

import { IconButton } from "@/components/IconButton";
import { Header } from "@/components/Header";
import { Separator } from "@/components/ui/separator";
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
import {
  useSidebar,
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupContent,
  SidebarInset,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarRail,
  SidebarTrigger,
} from "@/components/ui/sidebar";
import { TextField, TextFieldInput } from "@/components/ui/text-field";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { showToast } from "@/components/ui/toast";
import { DataTable } from "@/components/Table";

import type { QueryResponse } from "@bindings/QueryResponse";
import type { ListSchemasResponse } from "@bindings/ListSchemasResponse";
import type { SqlValue } from "@bindings/SqlValue";

import { createTableSchemaQuery } from "@/lib/api/table";
import { executeSql, type ExecutionResult } from "@/lib/api/execute";
import { isNotNull } from "@/lib/schema";
import { sqlValueToString } from "@/lib/value";
import { createIsMobile } from "@/lib/signals";
import type { ArrayRecord } from "@/lib/record";

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
  const isCached = () => props.response === undefined;
  const response = () => props.response ?? props.script.result;

  function columnDefs(data: QueryResponse): ColumnDef<ArrayRecord, SqlValue>[] {
    return (data.columns ?? []).map((col, idx) => {
      const notNull = isNotNull(col.options);

      const header = `${col.name} [${col.data_type}${notNull ? "" : "?"}]`;
      return {
        accessorFn: (row: ArrayRecord) => {
          return sqlValueToString(row[idx]);
        },
        header,
      };
    });
  }

  return (
    <Switch>
      <Match when={response()?.error}>
        <div class="p-4">Error: {response()?.error?.message}</div>
      </Match>

      <Match when={response()?.data === undefined}>
        <div class="p-4">No Data</div>
      </Match>

      <Match when={response()?.data !== undefined}>
        <ErrorBoundary
          fallback={(err, _reset) => {
            return (
              <div class="m-4 flex flex-col gap-4">
                <p>Failed to render query result: {`${err}`}</p>

                {isCached() && (
                  <p>
                    The view is trying to show cached data. Maybe the schema has
                    changed. Try to re-execute the query.
                  </p>
                )}
              </div>
            );
          }}
        >
          <div class="flex flex-col gap-2 p-4">
            <div class="flex justify-end text-sm">
              <ExecutionTime timestamp={response()?.timestamp} />
            </div>

            {/* TODO: Enable pagination */}
            <DataTable
              columns={() => columnDefs(response()!.data!)}
              data={() => response()!.data!.rows as ArrayRecord[]}
              pagination={{
                pageIndex: 0,
                pageSize: 50,
              }}
            />
          </div>
        </ErrorBoundary>
      </Match>
    </Switch>
  );
}

function ExecutionTime(props: { timestamp: number | undefined }) {
  const time = () => new Date(props.timestamp ?? 0);

  return (
    <div class="text-sm">{`Executed: ${time().toLocaleTimeString()}`}</div>
  );
}

function EditorSidebar(props: {
  selected: number;
  setSelected: (idx: number) => void;
  dirty: boolean;
  horizontal: boolean;
  deleteScriptByIdx: (idx: number) => void;
}) {
  const { setOpenMobile } = useSidebar();
  const scripts = useStore($scripts);

  const addNewScript = () => props.setSelected(createNewScript());

  return (
    <div class="p-2">
      <SidebarGroupContent>
        <SidebarMenu>
          <Button class="flex gap-2" variant="secondary" onClick={addNewScript}>
            <TbPencilPlus /> New
          </Button>

          <For each={scripts()}>
            {(script: Script, i: Accessor<number>) => {
              const scriptName = () => scripts()[i()].name;
              const showStar = () => props.selected === i() && props.dirty;

              return (
                <SidebarMenuItem>
                  <SidebarMenuButton
                    isActive={props.selected === i()}
                    tooltip={scriptName()}
                    class="pr-0"
                    variant="default"
                    size="md"
                    onClick={() => {
                      setOpenMobile(false);
                      props.setSelected(i());
                    }}
                  >
                    <div class="flex w-full items-center justify-between">
                      <span class="truncate">
                        {`${scriptName()}${showStar() ? "*" : ""}`}
                      </span>

                      <div class="flex">
                        <RenameDialog selected={i()} script={script} />

                        <IconButton
                          class="hover:bg-border"
                          tooltip="Delete this script"
                          onClick={() => props.deleteScriptByIdx(i())}
                        >
                          <TbTrash />
                        </IconButton>
                      </div>
                    </div>
                  </SidebarMenuButton>
                </SidebarMenuItem>
              );
            }}
          </For>
        </SidebarMenu>
      </SidebarGroupContent>
    </div>
  );
}

function HelpDialog() {
  return (
    <Dialog id="edit-help">
      <DialogTrigger>
        <IconButton>
          <TbHelp />
        </IconButton>
      </DialogTrigger>

      <DialogContent>
        <DialogHeader>
          <DialogTitle>Editor Help</DialogTitle>
        </DialogHeader>

        <p>
          The editor lets you execute arbitrary SQL statements. Be careful when
          experimenting, e.g. consider working on a non-prod data set or copy.
        </p>

        <p>{migrationWarning}</p>

        <p>
          Also note that there's no pagination. Selecting a large data set may
          return a lot of data. You might want to{" "}
          <span class="font-mono">LIMIT</span> your result size.
        </p>

        <p>
          Lastly, scripts are saved in your browser's local storage. This means
          switching devices, browsers or the origin of your website, you won't
          be able to access your scripts.{" "}
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
      <DialogTrigger>
        <IconButton tooltip="Rename script" class="hover:bg-border">
          <TbEdit />
        </IconButton>
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
  const [dirtyDialog, setDirtyDialog] = props.dirtyDialog;
  // eslint-disable-next-line solid/reactivity
  const [selected, setSelected] = props.selected;

  const uiState = useStore($uiState);

  const isMobile = createIsMobile();

  // Will only be set when the user explicitly triggers "execute";
  const [queryString, setQueryString] = createWritableMemo<string | null>(
    () => {
      // Rebuild whenever we switch selected scripts.
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
        executionResult.refetch();
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
    {
      key: "Ctrl-s",
      run: () => {
        saveScript();
        showToast({ title: "saved" });
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

  return (
    <Dialog
      id="switch-script-dialog"
      open={dirtyDialog() !== undefined}
      onOpenChange={(isOpen) => {
        if (!isOpen) {
          setDirtyDialog();
        }
      }}
      modal={true}
    >
      <ConfirmSwitchDialog
        back={() => setDirtyDialog()}
        confirm={() => {
          const state = dirtyDialog();
          if (state) {
            setDirtyDialog();

            setSelected(state.nextSelected);
            setDirty(false);
          }
        }}
        saveScript={saveScript}
        message="Proceeding will discard any pending changes in the current buffer. Proceed with caution."
      />

      <Header
        title="Editor"
        leading={<SidebarTrigger />}
        titleSelect={dirty() ? `${props.script.name}*` : props.script.name}
        right={<HelpDialog />}
      />

      <div class="mx-4 my-2 flex flex-col gap-2">
        {(uiState().showMigrationWarning ?? true) && (
          <Callout
            class="flex items-center text-sm hover:opacity-[80%]"
            onClick={() => {
              $uiState.set({
                ...uiState(),
                showMigrationWarning: false,
              });
            }}
          >
            <p>{migrationWarning}</p>

            <div class="p-2">
              <TbX size={20} />
            </div>
          </Callout>
        )}

        {/* Editor */}
        <div class="max-h-[40dvh] min-h-[6rem] shrink" ref={ref} />

        <div class="flex items-center justify-between">
          <Tooltip>
            <TooltipTrigger as="div">
              <Button variant="secondary" onClick={() => {}}>
                <Show when={!isMobile()} fallback="Save">
                  Save (Ctrl+S)
                </Show>
              </Button>
            </TooltipTrigger>

            <TooltipContent>
              Save script to browser local storage.
            </TooltipContent>
          </Tooltip>

          <Tooltip>
            <TooltipTrigger as="div">
              <Button variant="destructive" onClick={execute}>
                <Show when={!isMobile()} fallback="Execute">
                  Execute (Ctrl+Enter)
                </Show>
              </Button>
            </TooltipTrigger>

            <TooltipContent>
              Execute script on the server. No turning back.
            </TooltipContent>
          </Tooltip>
        </div>
      </div>

      <Separator />

      <div class="flex flex-col">
        <ResultView
          script={props.script}
          response={executionResult.data ?? undefined}
        />
      </div>
    </Dialog>
  );
}

export function EditorPage() {
  // FIXME: Note that the state isn't persistent enough. E.g. resizing to
  // mobile rebuild EditorPage and reset the dirty state.
  const scripts = useStore($scripts);
  const [selected, setSelected] = createSignal<number>(0);
  const [dirty, setDirty] = createSignal<boolean>(false);
  const isMobile = createIsMobile();

  const [dirtyDialog, setDirtyDialog] = createSignal<
    DirtyDialogState | undefined
  >();
  const switchToScript = (idx: number) => {
    if (dirty()) {
      setDirtyDialog({ nextSelected: idx });
    } else {
      setSelected(idx);
    }
  };

  const schemaFetch = createTableSchemaQuery();

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

  const deleteScriptByIdx = (idx?: number | undefined) => {
    const i = idx ?? selected();
    deleteScript(i);
    setSelected(Math.max(0, i - 1));
  };

  return (
    <SidebarProvider>
      <Sidebar
        class="absolute"
        variant="sidebar"
        side="left"
        collapsible="offcanvas"
      >
        <SidebarContent>
          <SidebarGroup>
            <EditorSidebar
              selected={selected()}
              setSelected={switchToScript}
              dirty={dirty()}
              horizontal={true}
              deleteScriptByIdx={deleteScriptByIdx}
            />
          </SidebarGroup>

          {/* <SidebarFooter /> */}
        </SidebarContent>

        <SidebarRail />
      </Sidebar>

      <SidebarInset class="min-w-0">
        <Switch fallback={"Loading..."}>
          <Match when={schemaFetch.isError}>
            <span>Schema fetch error: {JSON.stringify(schemaFetch.error)}</span>
          </Match>

          <Match when={schemaFetch.data && isMobile()}>
            <EditorPanel
              schemas={schemaFetch.data!}
              selected={[selected, setSelected]}
              script={script()}
              dirty={[dirty, setDirty]}
              dirtyDialog={[dirtyDialog, setDirtyDialog]}
              deleteScript={() => deleteScriptByIdx()}
            />
          </Match>

          <Match when={schemaFetch.data && !isMobile()}>
            <div class="h-dvh overflow-y-auto">
              <EditorPanel
                schemas={schemaFetch.data!}
                selected={[selected, setSelected]}
                script={script()}
                dirty={[dirty, setDirty]}
                dirtyDialog={[dirtyDialog, setDirtyDialog]}
                deleteScript={() => deleteScriptByIdx()}
              />
            </div>
          </Match>
        </Switch>
      </SidebarInset>
    </SidebarProvider>
  );
}

const myTheme = EditorView.theme(
  {
    ".cm-gutters": {
      backgroundColor: "#f3f7f9",
      color: "#000",
      border: "none",
      borderRadius: "8px 0px 0px 8px",
    },
    "&.cm-editor": {
      outline: "1px solid #e4e4e7",
      borderRadius: "8px",
    },
    // "&.cm-editor.cm-focused": {
    //   outline: "1px solid gray",
    //   borderRadius: "8px",
    // },
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

type UiState = {
  showMigrationWarning?: boolean;
};

const $uiState = persistentAtom<UiState>(
  "editor_ui_state",
  {},
  {
    encode: JSON.stringify,
    decode: JSON.parse,
  },
);

const migrationWarning =
  "\
When changing schemas, consider using migrations for \
cross-deployment consistency (dev, test, prod, etc.) One-off changes \
may lead to skew. Alterations using the table browser will yield migrations.";

// Needed for lazy load.
export default EditorPage;
