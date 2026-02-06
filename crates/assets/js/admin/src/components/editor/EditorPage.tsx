import {
  ErrorBoundary,
  For,
  Match,
  Show,
  Switch,
  createMemo,
  createEffect,
  createSignal,
  onCleanup,
} from "solid-js";
import type { Accessor, Signal } from "solid-js";
import { useQuery } from "@tanstack/solid-query";
import { createWritableMemo } from "@solid-primitives/memo";
import type { ColumnDef } from "@tanstack/solid-table";
import { persistentAtom } from "@nanostores/persistent";
import { useStore } from "@nanostores/solid";
import {
  TbOutlineTrash,
  TbOutlineEdit,
  TbOutlineHelp,
  TbOutlinePencilPlus,
  TbOutlineX,
} from "solid-icons/tb";

import { autocompletion } from "@codemirror/autocomplete";
import { EditorView, lineNumbers, keymap } from "@codemirror/view";
import { EditorState } from "@codemirror/state";
import { minimalSetup } from "codemirror";
import { sql, SQLConfig, SQLNamespace, SQLite } from "@codemirror/lang-sql";

import { IconButton } from "@/components/IconButton";
import { Header } from "@/components/Header";
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
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
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
import { Table, buildTable } from "@/components/Table";
import { useNavbar, DirtyDialog } from "@/components/Navbar";

import type { QueryResponse } from "@bindings/QueryResponse";
import type { ListSchemasResponse } from "@bindings/ListSchemasResponse";
import type { SqlValue } from "@bindings/SqlValue";

import { createConfigQuery } from "@/lib/api/config";
import { createTableSchemaQuery } from "@/lib/api/table";
import { executeSql, type ExecutionResult } from "@/lib/api/execute";
import { isNotNull } from "@/lib/schema";
import { sqlValueToString } from "@/lib/value";
import { prettyFormatQualifiedName } from "@/lib/schema";
import { createIsMobile } from "@/lib/signals";
import type { ArrayRecord } from "@/lib/record";

function buildSchema(schemas: ListSchemasResponse): SQLNamespace {
  const schema: {
    [name: string]: SQLNamespace;
  } = {};

  for (const [table, _] of schemas.tables) {
    const tableName = prettyFormatQualifiedName(table.name);
    schema[tableName] = {
      self: { label: tableName, type: "keyword" },
      children: table.columns.map((c) => c.name),
    } satisfies SQLNamespace;
  }

  for (const [view, _] of schemas.views) {
    const viewName = prettyFormatQualifiedName(view.name);
    schema[viewName] = {
      self: { label: viewName, type: "keyword" },
      children: view.column_mapping?.columns.map((c) => c.column.name) ?? [],
    } satisfies SQLNamespace;
  }

  return schema;
}

function ResultView(props: {
  script: Script;
  response: ExecutionResult | undefined;
}) {
  const isCached = () => props.response === undefined;
  const response = () => props.response ?? props.script.result;

  return (
    <Switch>
      <Match when={response()?.error}>
        <div class="flex flex-col gap-2 p-4">
          <div class="flex justify-end text-sm">
            <ExecutionTime timestamp={response()?.timestamp} />
          </div>
          Error: {response()?.error?.message}
        </div>
      </Match>

      <Match when={response()?.data === undefined}>
        <div class="flex flex-col gap-2 p-4">
          <div class="flex justify-end text-sm">
            <ExecutionTime timestamp={response()?.timestamp} />
          </div>
          No data
        </div>
      </Match>

      <Match when={response()?.data !== undefined}>
        <ResultViewImpl
          data={response()!.data!}
          timestamp={response()?.timestamp}
          isCached={isCached()}
        />
      </Match>
    </Switch>
  );
}

function ResultViewImpl(props: {
  data: QueryResponse;
  isCached: boolean;
  timestamp?: number;
}) {
  const [columnPinningState, setColumnPinningState] = createSignal({});

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

  const dataTable = createMemo(() => {
    // TODO: Enable pagination
    return buildTable({
      columns: columnDefs(props.data),
      data: props.data.rows,
      columnPinning: columnPinningState,
      onColumnPinningChange: setColumnPinningState,
    });
  });

  return (
    <ErrorBoundary
      fallback={(err, _reset) => {
        return (
          <div class="m-4 flex flex-col gap-4">
            <p>Failed to render query result: {`${err}`}</p>

            <Show when={props.isCached}>
              <p>
                The view is trying to show cached data. Maybe the schema has
                changed. Try to re-execute the query.
              </p>
            </Show>
          </div>
        );
      }}
    >
      <div class="flex flex-col gap-2 p-4">
        <div class="flex justify-end text-sm">
          <ExecutionTime timestamp={props.timestamp} />
        </div>

        <Table table={dataTable()} loading={false} />
      </div>
    </ErrorBoundary>
  );
}

function ExecutionTime(props: { timestamp: number | undefined }) {
  const time = () => new Date(props.timestamp ?? 0);

  return <div class="text-sm">{`Executed: ${time().toLocaleString()}`}</div>;
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
          <Button
            class="flex gap-2"
            variant="secondary"
            onClick={() => {
              setOpenMobile(false);
              addNewScript();
            }}
          >
            <TbOutlinePencilPlus /> New
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
                          onClick={(e) => {
                            props.deleteScriptByIdx(i());
                            e.stopPropagation();
                          }}
                        >
                          <TbOutlineTrash />
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
          <TbOutlineHelp />
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

  let ref: HTMLInputElement | undefined;

  return (
    <Dialog id="script-rename-dialog" open={open()} onOpenChange={setOpen}>
      <DialogTrigger
        onClick={(e) => {
          e.stopPropagation();
        }}
      >
        <IconButton tooltip="Rename script" class="hover:bg-border">
          <TbOutlineEdit />
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

            const name = ref?.value;
            if (name !== undefined) {
              updateExistingScript(props.selected, {
                ...props.script,
                name,
              });
              setOpen(false);
            }
          }}
        >
          <TextField>
            <TextFieldInput
              ref={ref}
              required={true}
              pattern=".+"
              value={props.script.name}
              type="text"
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
  const config = createConfigQuery();

  const isMobile = createIsMobile();

  const databases = () =>
    config.data?.config?.databases
      .map((db) => db.name)
      .filter((n) => n !== undefined);

  const [attachedDbs, setAttachedDbs] = createSignal<string[]>(
    databases()?.slice(0, 124) ?? [],
  );
  const [queryString, setQueryString] = createWritableMemo<string | null>(
    () => {
      // Reset queryString to null whenever we switch scripts. If we read query
      // string from the editor contents, useQuery would eagerly run the query.
      // Instead we don't want to run new scripts right away, null short-circuits the fetch.
      return selected() ? null : null;
    },
  );

  const executionResult = useQuery(() => {
    return {
      // Consider initial data fresh enough.
      staleTime: 1000 * 7400,
      initialData: props.script.result,
      // Just keying on query isn't enough, since multiple tabs/scripts may
      // have the same contents.
      queryKey: [
        { index: selected(), query: queryString(), attachedDbs: attachedDbs() },
      ],
      queryFn: async ({ queryKey }) => {
        const [{ query, attachedDbs }] = queryKey;
        if (query === null) {
          return null;
        }

        const response = await executeSql(
          query,
          attachedDbs.length > 0 ? attachedDbs : null,
        );
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

  let ref: HTMLDivElement | undefined;
  let editor: EditorView | undefined;

  onCleanup(() => editor?.destroy());
  createEffect(() => {
    const newEditorState = (contents: string) => {
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
            return true;
          },
          preventDefault: true,
        },
      ]);

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

    // Every time the script contents change, recreate the editor state.
    editor?.destroy();
    editor = new EditorView({
      parent: ref!,
      state: newEditorState(props.script.contents),
    });
    editor.focus();
  });

  const execute = () => {
    const query = editor?.state.doc.toString();
    if (query !== undefined) {
      setQueryString(query);
      executionResult.refetch();
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
    showToast({ title: "saved" });
  };

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
      <DirtyDialog
        back={() => setDirtyDialog()}
        proceed={() => {
          const state = dirtyDialog();
          if (state) {
            setDirtyDialog();

            setSelected(state.nextSelected);
            setDirty(false);
          }
        }}
        save={saveScript}
      />

      <Header
        title="Editor"
        leading={<SidebarTrigger />}
        titleSelect={dirty() ? `${props.script.name}*` : props.script.name}
        right={
          <div class="flex items-center">
            <Select<string>
              multiple={true}
              options={[...(databases() ?? [])]}
              value={attachedDbs()}
              itemComponent={(props) => (
                <SelectItem item={props.item}>{props.item.rawValue}</SelectItem>
              )}
              onChange={(value: string[]) => setAttachedDbs(value)}
            >
              <div class="flex items-center gap-2">
                Attached
                <SelectTrigger>
                  <SelectValue class="max-w-[50%] min-w-[32px] text-ellipsis">
                    {(state) => {
                      const selected = state.selectedOptions();
                      if (selected.length === 0) {
                        // FIXME: state callback never gets called when empty.
                        return "none";
                      }
                      return selected.join(", ");
                    }}
                  </SelectValue>
                </SelectTrigger>
              </div>

              <SelectContent />
            </Select>

            <HelpDialog />
          </div>
        }
      />

      <div class="mx-4 my-2 flex flex-col gap-2">
        {(uiState().showMigrationWarning ?? true) && (
          <Callout
            class="flex items-center text-sm hover:opacity-80"
            onClick={() => {
              $uiState.set({
                ...uiState(),
                showMigrationWarning: false,
              });
            }}
          >
            <p>{migrationWarning}</p>

            <div class="p-2">
              <TbOutlineX size={20} />
            </div>
          </Callout>
        )}

        {/* Editor */}
        <div class="min-h-24 shrink" ref={ref} />

        <div class="flex items-center justify-between">
          <Tooltip>
            <TooltipTrigger as="div">
              <Button variant="secondary" onClick={() => saveScript()}>
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

      <ResultView
        script={props.script}
        response={executionResult.data ?? undefined}
      />
    </Dialog>
  );
}

export function EditorPage() {
  // FIXME: Note that the state isn't persistent enough. E.g. resizing to
  // mobile rebuild EditorPage and reset the dirty state.
  const scripts = useStore($scripts);
  const isMobile = createIsMobile();
  const [selected, setSelected] = createSignal<number>(0);
  const [dirty, setDirty] = createSignal<boolean>(false);

  const navbar = useNavbar();
  createEffect(() => {
    navbar?.setDirty(dirty());
  });

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
    <SidebarProvider class="min-h-0">
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

      <SidebarInset class="min-h-0 min-w-0">
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
