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
  untrack,
} from "solid-js";
import type { Accessor, Signal } from "solid-js";
import { useQuery } from "@tanstack/solid-query";
import type { DefinedUseQueryResult } from "@tanstack/solid-query";
import { createWritableMemo } from "@solid-primitives/memo";
import type { ColumnDef } from "@tanstack/solid-table";
import { persistentAtom } from "@nanostores/persistent";
import { useStore } from "@nanostores/solid";
import {
  TbOutlineTrash,
  TbOutlineEdit,
  TbOutlineHelp,
  TbOutlinePencilPlus,
  TbOutlineCopy,
} from "solid-icons/tb";

import { autocompletion } from "@codemirror/autocomplete";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { EditorView, lineNumbers, keymap } from "@codemirror/view";
import { EditorState } from "@codemirror/state";
import { minimalSetup } from "codemirror";
import { sql, SQLConfig, SQLNamespace, SQLite } from "@codemirror/lang-sql";
import { tags } from "@lezer/highlight";

import { IconButton } from "@/components/IconButton";
import { Header } from "@/components/Header";
import { Badge } from "@/components/ui/badge";
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
  SidebarHeader,
  SidebarInset,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarRail,
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
import { createTheme } from "@/lib/theme";
import { createTableSchemaQuery } from "@/lib/api/table";
import { executeSql, type ExecutionResult } from "@/lib/api/execute";
import { isNotNull } from "@/lib/schema";
import { copyToClipboard } from "@/lib/utils";
import { sqlValueToString } from "@/lib/value";
import { prettyFormatQualifiedName } from "@/lib/schema";
import { createIsMobile } from "@/lib/signals";
import type { ArrayRecord } from "@/lib/record";

type SimpleSignal<T> = [Accessor<T>, set: (state: T) => void];

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

function buildCsv(response: QueryResponse): string {
  function escapeCsv(v: string): string {
    return `"${v.replaceAll('"', '""')}"`;
  }

  const lines: string[] = [];

  const columns = response.columns;
  if (columns !== null) {
    lines.push(columns.map((c) => escapeCsv(c.name)).join(", "));
  }

  for (const row of response.rows) {
    lines.push(row.map((v) => escapeCsv(sqlValueToString(v))).join(", "));
  }

  return lines.join("\n");
}

function ResultsHeader(props: {
  query: DefinedUseQueryResult<ExecutionResult | null | undefined, Error>;
}) {
  const data = () => props.query?.data?.data;
  const timestamp = () => props.query?.data?.timestamp;

  return (
    <div class="flex items-center justify-between gap-2 text-sm">
      <div class="flex items-center gap-2">
        <Switch>
          <Match when={props.query.isPending}>
            <Badge variant="warning">Running...</Badge>
          </Match>

          <Match when={props.query.isError || props.query.data?.error}>
            <Badge variant="error">Error</Badge>
          </Match>

          <Match when={true}>
            <Badge variant="success">Ok</Badge>
          </Match>
        </Switch>

        <Button
          variant="ghost"
          size="icon"
          disabled={data() === undefined}
          onClick={() => {
            const current = data();
            if (current !== undefined) {
              copyToClipboard(buildCsv(current));
            }
          }}
        >
          <TbOutlineCopy />
        </Button>
      </div>

      <ExecutionTime timestamp={timestamp()} />
    </div>
  );
}

function ResultView(props: {
  script: Script;
  query: DefinedUseQueryResult<ExecutionResult | null | undefined, Error>;
}) {
  const isCached = () => props.query?.data === undefined;
  const response = () => props.query?.data ?? props.script.result;

  return (
    <div class="flex flex-col gap-2 p-4">
      <ResultsHeader query={props.query} />

      <Switch>
        <Match when={response()?.error}>
          Error: {response()?.error?.message}
        </Match>

        <Match when={response()?.data === undefined}>No data</Match>

        <Match when={response()?.data !== undefined}>
          <ResultViewImpl
            data={response()!.data!}
            timestamp={response()?.timestamp}
            isCached={isCached()}
          />
        </Match>
      </Switch>
    </div>
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
      <Table table={dataTable()} loading={false} />
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
    <>
      <SidebarHeader>
        <div class="flex justify-between gap-2 p-2">
          <div>
            <h3>Saved queries</h3>
            <span class="text-xs">{scripts().length} saved</span>
          </div>

          <Tooltip>
            <TooltipTrigger as="div">
              <Button
                class="flex gap-2"
                variant="ghost"
                size="icon"
                onClick={() => {
                  setOpenMobile(false);
                  addNewScript();
                }}
              >
                <TbOutlinePencilPlus />
              </Button>
            </TooltipTrigger>

            <TooltipContent>Add new script.</TooltipContent>
          </Tooltip>
        </div>
      </SidebarHeader>

      <SidebarContent class="px-2">
        <SidebarGroup>
          <SidebarGroupContent>
            <SidebarMenu>
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
        </SidebarGroup>
      </SidebarContent>
    </>
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
  selected: SimpleSignal<number>;
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

  const config = createConfigQuery();

  const isMobile = createIsMobile();
  const theme = createTheme();

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

  type SchemaChanges = {
    allow: boolean;
    showDialog: boolean;
  };
  const [schemaChanges, setSchemaChanges] = createWritableMemo<SchemaChanges>(
    () => {
      const def = (): SchemaChanges => ({ allow: false, showDialog: true });
      // Reset whenever script changes.
      return selected() ? def() : def();
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
          /* allowSchemaAlteration= */ untrack(() => schemaChanges().allow),
        );

        const error = response.error;
        if (error && error.code !== 412) {
          showToast({
            title: `Execution Error (${error.code})`,
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
          key: "Mod-Enter",
          run: () => {
            execute();
            return true;
          },
          preventDefault: true,
        },
        {
          key: "Mod-s",
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
          editorTheme(theme() === "dark"),
          theme() === "dark" ? darkSqlSyntaxHighlighting : [],
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
      // Reset.
      setQueryString(query);

      // Then execute.
      (async () => {
        await executionResult.refetch();

        // Allow showing the dialog again if the user refused the first time.
        setSchemaChanges((s) => ({ ...s, showDialog: true }));
      })();
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

      {/* nested dialog for schema changes */}
      <Dialog
        id="allow-schema-changes-dialog"
        open={
          executionResult.data?.error?.code === 412 &&
          schemaChanges().showDialog
        }
        onOpenChange={(isOpen) => {
          if (!isOpen) {
            setSchemaChanges((s) => ({ ...s, showDialog: false }));
          }
        }}
        modal={true}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Schema Change Detected</DialogTitle>
          </DialogHeader>

          <p>{migrationWarning}</p>

          <p>Do you want to continue w/o a migration?</p>

          <div class="flex justify-between gap-2">
            <Button
              variant="outline"
              onClick={() =>
                setSchemaChanges((s) => ({ ...s, showDialog: false }))
              }
            >
              back
            </Button>

            <Button
              variant="destructive"
              onClick={() => {
                setSchemaChanges({ showDialog: false, allow: true });
                execute();
              }}
            >
              Proceed
            </Button>
          </div>
        </DialogContent>

        <Header
          title="SQL Edit"
          titleSelect={dirty() ? `${props.script.name}*` : props.script.name}
          right={
            <div class="flex items-center">
              <Select<string>
                multiple={true}
                options={[...(databases() ?? [])]}
                value={attachedDbs()}
                itemComponent={(props) => (
                  <SelectItem item={props.item}>
                    {props.item.rawValue}
                  </SelectItem>
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
          {/* Editor container */}
          <div class="min-h-24 shrink">
            <div ref={ref} />
          </div>

          <div class="flex items-center justify-between">
            <Tooltip>
              <TooltipTrigger as="div">
                <Button variant="secondary" onClick={() => saveScript()}>
                  <Show when={!isMobile()} fallback="Save">
                    Save ({`${modKey}+S`})
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
                    Execute ({`${modKey}+Enter`})
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

        <ResultView script={props.script} query={executionResult} />
      </Dialog>
    </Dialog>
  );
}

export function EditorPage() {
  // FIXME: Note that the state isn't persistent enough. E.g. resizing to
  // mobile rebuild EditorPage and reset the dirty state.
  const scripts = useStore($scripts);
  const [dirty, setDirty] = createSignal<boolean>(false);

  const [selected, setSelectedImpl] = createSignal<number>(
    $uiState.get().selected ?? 0,
  );
  const setSelected = (idx: number) => {
    $uiState.set({
      ...$uiState.get(),
      selected: idx,
    });
    return setSelectedImpl(idx);
  };

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
    if (s.length === 0) {
      return defaultScript;
    }

    const i = idx ?? selected();
    return s[i < s.length ? i : s.length - 1];
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
        <EditorSidebar
          selected={selected()}
          setSelected={switchToScript}
          dirty={dirty()}
          horizontal={true}
          deleteScriptByIdx={deleteScriptByIdx}
        />

        <SidebarRail />
      </Sidebar>

      <SidebarInset>
        <Switch fallback={"Loading..."}>
          <Match when={schemaFetch.isError}>
            <span>Schema fetch error: {JSON.stringify(schemaFetch.error)}</span>
          </Match>

          <Match when={schemaFetch.data}>
            <div class="flex size-full flex-col">
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

function editorTheme(dark: boolean) {
  return EditorView.theme(
    {
      ".cm-gutters": {
        backgroundColor: dark ? "#000" : "#f3f7f9",
        color: dark ? "#FFFFFF" : "#000",
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
    { dark },
  );
}

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

export const DARK_SQL_COLORS = {
  keyword: "#7dd3fc",
  string: "#86efac",
  number: "#fde68a",
  comment: "#a1a1aa",
  name: "#e4e4e7",
  operator: "#f9a8d4",
  punctuation: "#cbd5e1",
  invalid: "#fca5a5",
} as const;

const darkSqlSyntaxHighlighting = syntaxHighlighting(
  HighlightStyle.define([
    { tag: tags.keyword, color: DARK_SQL_COLORS.keyword, fontWeight: "600" },
    {
      tag: [tags.string, tags.special(tags.string)],
      color: DARK_SQL_COLORS.string,
    },
    {
      tag: [tags.number, tags.bool, tags.atom],
      color: DARK_SQL_COLORS.number,
    },
    {
      tag: [tags.comment, tags.lineComment, tags.blockComment],
      color: DARK_SQL_COLORS.comment,
      fontStyle: "italic",
    },
    {
      tag: [tags.variableName, tags.propertyName, tags.typeName],
      color: DARK_SQL_COLORS.name,
    },
    {
      tag: [tags.operator, tags.compareOperator],
      color: DARK_SQL_COLORS.operator,
    },
    {
      tag: [tags.punctuation, tags.separator],
      color: DARK_SQL_COLORS.punctuation,
    },
    { tag: tags.invalid, color: DARK_SQL_COLORS.invalid },
  ]),
);

const $scripts = persistentAtom<Script[]>("scripts", [defaultScript], {
  encode: JSON.stringify,
  decode: JSON.parse,
});

type UiState = {
  selected?: number;
};

const $uiState = persistentAtom<UiState>(
  "editor_ui_state",
  {},
  {
    encode: JSON.stringify,
    decode: JSON.parse,
  },
);

const modKey = "Ctrl/⌘";
const migrationWarning =
  "\
When changing schemas, consider using migrations for \
cross-deployment consistency (dev, test, prod, etc.) One-off changes \
may lead to skew. Alterations using the table browser will yield migrations.";

// Needed for lazy load.
export default EditorPage;
