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
  TbOutlineCopy,
  TbOutlineDotsVertical,
} from "solid-icons/tb";

import { autocompletion } from "@codemirror/autocomplete";
import { EditorView, lineNumbers, keymap } from "@codemirror/view";
import { EditorState } from "@codemirror/state";
import { minimalSetup } from "codemirror";
import { sql, SQLConfig, SQLNamespace, SQLite } from "@codemirror/lang-sql";

import { IconButton } from "@/components/IconButton";
import { Spinner } from "@/components/Spinner";
import { Callout } from "@/components/ui/callout";
import { Button, buttonVariants } from "@/components/ui/button";
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
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { showToast } from "@/components/ui/toast";
import { Table, buildTable } from "@/components/Table";
import { useNavbar, DirtyDialog } from "@/components/Navbar";

import type { QueryResponse } from "@bindings/QueryResponse";
import type { ListSchemasResponse } from "@bindings/ListSchemasResponse";
import type { SqlValue } from "@bindings/SqlValue";

import { createConfigQuery } from "@/lib/api/config";
import { currentTheme } from "@/lib/theme";
import { createTableSchemaQuery } from "@/lib/api/table";
import { executeSql, type ExecutionResult } from "@/lib/api/execute";
import { isNotNull } from "@/lib/schema";
import { copyToClipboard } from "@/lib/utils";
import { sqlValueToString } from "@/lib/value";
import { prettyFormatQualifiedName } from "@/lib/schema";
import { createIsMobile } from "@/lib/signals";
import type { ArrayRecord } from "@/lib/record";

type SimpleSignal<T> = [Accessor<T>, set: (state: T) => void];

export function filterSavedQueries(
  scripts: Script[],
  search: string,
): Script[] {
  const query = search.trim().toLocaleLowerCase();
  return query
    ? scripts.filter((script) =>
        script.name.toLocaleLowerCase().includes(query),
      )
    : scripts;
}

export function paginateResultRows<T>(
  rows: T[],
  pageIndex: number,
  pageSize: number,
): T[] {
  const start = pageIndex * pageSize;
  return rows.slice(start, start + pageSize);
}

export function resultPresentation(
  result: ExecutionResult | undefined,
  cached: boolean,
  running = false,
): { label: string } {
  if (running) return { label: "Running…" };
  if (!result) return { label: "No result" };
  if (result.error) return { label: "Error" };
  if (cached) return { label: "Cached result" };
  if (result.data?.columns === null) return { label: "No data" };
  if (result.data?.rows.length === 0) return { label: "No rows" };
  return { label: "Success" };
}

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

export function buildCsv(response: QueryResponse): string {
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
  data: QueryResponse | undefined;
  timestamp: number | undefined;
  status: string;
}) {
  return (
    <div class="flex items-center justify-between gap-2 text-sm">
      <div class="flex items-center gap-2">
        <span class="font-medium">{props.status}</span>
        <Button
          variant="ghost"
          size="icon"
          aria-label="Copy results as CSV"
          disabled={props.data === undefined}
          onClick={() => {
            const data = props.data;
            if (data !== undefined) {
              copyToClipboard(buildCsv(data));
            }
          }}
        >
          <TbOutlineCopy />
        </Button>
      </div>

      <ExecutionTime timestamp={props.timestamp} />
    </div>
  );
}

function ResultView(props: {
  script: Script;
  response: ExecutionResult | undefined;
  running: boolean;
}) {
  const isCached = () => props.response === undefined;
  const response = () => props.response ?? props.script.result;
  const status = () =>
    resultPresentation(response(), isCached(), props.running).label;

  return (
    <Switch>
      <Match when={response()?.error}>
        <div class="flex flex-col gap-2 p-4">
          <ResultsHeader
            data={response()?.data}
            timestamp={response()?.timestamp}
            status={status()}
          />
          Error: {response()?.error?.message}
        </div>
      </Match>

      <Match when={response()?.data === undefined}>
        <div class="flex flex-col gap-2 p-4">
          <ResultsHeader
            data={response()?.data}
            timestamp={response()?.timestamp}
            status={status()}
          />
          No data
        </div>
      </Match>

      <Match when={response()?.data !== undefined}>
        <ResultViewImpl
          data={response()!.data!}
          timestamp={response()?.timestamp}
          isCached={isCached()}
          status={status()}
        />
      </Match>
    </Switch>
  );
}

function ResultViewImpl(props: {
  data: QueryResponse;
  isCached: boolean;
  status: string;
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
        <ResultsHeader
          data={props.data}
          timestamp={props.timestamp}
          status={props.status}
        />

        <Table table={dataTable()} loading={false} />
      </div>
    </ErrorBoundary>
  );
}

function ExecutionTime(props: { timestamp: number | undefined }) {
  const time = () => new Date(props.timestamp ?? 0);

  return <div class="text-sm">{`Executed: ${time().toLocaleString()}`}</div>;
}

export function EditorSidebar(props: {
  selected: number;
  setSelected: (idx: number) => void;
  dirty: boolean;
  deleteScriptByIdx: (idx: number) => void;
}) {
  const { setOpenMobile } = useSidebar();
  const scripts = useStore($scripts);
  const [search, setSearch] = createSignal("");
  const filteredScripts = createMemo(() =>
    filterSavedQueries(scripts(), search()),
  );

  const addNewScript = () => {
    setOpenMobile(false);
    props.setSelected(createNewScript());
  };

  return (
    <div class="flex h-full min-h-0 flex-col gap-3 p-2">
      <div class="flex items-start justify-between gap-2 px-1 pt-1">
        <div>
          <h2 class="text-sm font-semibold">Saved queries</h2>
          <p class="text-muted-foreground text-xs">
            {search().trim()
              ? `${filteredScripts().length} of ${scripts().length}`
              : `${scripts().length} saved`}
          </p>
        </div>
        <Button
          variant="ghost"
          size="icon"
          class="size-8"
          aria-label="Create query"
          onClick={addNewScript}
        >
          <TbOutlinePencilPlus />
        </Button>
      </div>

      <TextField>
        <TextFieldInput
          type="search"
          aria-label="Search saved queries"
          placeholder="Search saved queries"
          value={search()}
          onInput={(event) => setSearch(event.currentTarget.value)}
        />
      </TextField>

      <SidebarGroupContent class="min-h-0 overflow-y-auto">
        <SidebarMenu>
          <Show
            when={scripts().length > 0}
            fallback={
              <div class="text-muted-foreground flex flex-col items-center gap-3 px-3 py-8 text-center text-sm">
                <p>No saved queries yet</p>
                <Button variant="secondary" onClick={addNewScript}>
                  Create query
                </Button>
              </div>
            }
          >
            <Show
              when={filteredScripts().length > 0}
              fallback={
                <div class="text-muted-foreground flex flex-col items-center gap-3 px-3 py-8 text-center text-sm">
                  <p>No saved queries match</p>
                  <Button variant="ghost" onClick={() => setSearch("")}>
                    Clear search
                  </Button>
                </div>
              }
            >
              <For each={filteredScripts()}>
                {(script: Script) => {
                  const index = () => scripts().indexOf(script);
                  const selected = () => props.selected === index();
                  const dirty = () => selected() && props.dirty;

                  return (
                    <SidebarMenuItem class="flex items-center gap-1">
                      <SidebarMenuButton
                        isActive={selected()}
                        tooltip={script.name}
                        class="min-w-0 flex-1"
                        variant="default"
                        size="md"
                        onClick={() => {
                          setOpenMobile(false);
                          props.setSelected(index());
                        }}
                      >
                        <span class="truncate">{script.name}</span>
                        <Show when={dirty()}>
                          <span class="text-primary ml-auto" aria-hidden="true">
                            •
                          </span>
                          <span class="sr-only">Unsaved changes</span>
                        </Show>
                      </SidebarMenuButton>

                      <QueryActions
                        selected={index()}
                        script={script}
                        deleteScript={() => props.deleteScriptByIdx(index())}
                      />
                    </SidebarMenuItem>
                  );
                }}
              </For>
            </Show>
          </Show>
        </SidebarMenu>
      </SidebarGroupContent>
    </div>
  );
}

function HelpDialog() {
  return (
    <Dialog id="edit-help">
      <DialogTrigger>
        <IconButton tooltip="SQL editor help">
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

function QueryActions(props: {
  selected: number;
  script: Script;
  deleteScript: () => void;
}) {
  const [renameOpen, setRenameOpen] = createSignal(false);
  const [deleteOpen, setDeleteOpen] = createSignal(false);

  return (
    <>
      <DropdownMenu placement="bottom-end">
        <DropdownMenuTrigger
          class={buttonVariants({ variant: "ghost", size: "icon" })}
          aria-label={`Actions for ${props.script.name}`}
        >
          <TbOutlineDotsVertical />
        </DropdownMenuTrigger>
        <DropdownMenuContent>
          <DropdownMenuItem onSelect={() => setRenameOpen(true)}>
            <TbOutlineEdit />
            Rename
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem
            class="text-destructive ui-highlighted:text-destructive"
            onSelect={() => setDeleteOpen(true)}
          >
            <TbOutlineTrash />
            Delete
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

      <RenameDialog
        open={renameOpen()}
        onOpenChange={setRenameOpen}
        selected={props.selected}
        script={props.script}
      />

      <Dialog open={deleteOpen()} onOpenChange={setDeleteOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete query?</DialogTitle>
          </DialogHeader>
          <p class="text-muted-foreground text-sm">
            “{props.script.name}” and its cached result will be removed from
            this browser.
          </p>
          <DialogFooter>
            <Button variant="outline" onClick={() => setDeleteOpen(false)}>
              Cancel
            </Button>
            <Button
              variant="destructive"
              onClick={() => {
                props.deleteScript();
                setDeleteOpen(false);
              }}
            >
              Delete query
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

function RenameDialog(props: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  selected: number;
  script: Script;
}) {
  let ref: HTMLInputElement | undefined;

  return (
    <Dialog
      id="script-rename-dialog"
      open={props.open}
      onOpenChange={props.onOpenChange}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Rename query</DialogTitle>
        </DialogHeader>

        <form
          class="flex flex-col gap-4"
          method="dialog"
          onSubmit={(e: SubmitEvent) => {
            e.preventDefault();

            const name = ref?.value.trim();
            if (name) {
              updateExistingScript(props.selected, {
                ...props.script,
                name,
              });
              props.onOpenChange(false);
            }
          }}
        >
          <TextField>
            <TextFieldInput
              ref={ref}
              aria-label="Query name"
              required={true}
              pattern=".+"
              value={props.script.name}
              type="text"
            />
          </TextField>

          <DialogFooter>
            <Button type="submit">Rename query</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

export function QueryActionBar(props: {
  busy: boolean;
  mobile: boolean;
  onSave: () => void;
  onExecute: () => void;
}) {
  return (
    <div class="flex items-center justify-end gap-2">
      <Button variant="outline" aria-label="Save query" onClick={props.onSave}>
        {props.mobile ? "Save" : "Save (Ctrl/⌘+S)"}
      </Button>
      <Button
        aria-label="Execute query"
        disabled={props.busy}
        onClick={props.onExecute}
      >
        <Show
          when={!props.busy}
          fallback={
            <>
              <Spinner class="size-4" size={16} />
              Running…
            </>
          }
        >
          {props.mobile ? "Execute" : "Execute (Ctrl/⌘+Enter)"}
        </Show>
      </Button>
    </div>
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

  const uiState = useStore($uiState);
  const config = createConfigQuery();

  const isMobile = createIsMobile();
  const { state: explorerState } = useSidebar();

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
          editorTheme(currentTheme() === "dark"),
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
    if (executionResult.isFetching) return;
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
    showToast({ title: "Query saved" });
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

      <header class="bg-background/95 sticky top-0 z-20 border-b backdrop-blur-sm">
        <div class="flex min-h-14 flex-wrap items-center justify-between gap-2 px-3 py-2 sm:px-4">
          <div class="flex min-w-0 items-center gap-2">
            <SidebarTrigger
              aria-label={
                explorerState() === "collapsed"
                  ? "Show saved queries"
                  : "Hide saved queries"
              }
            />
            <div class="flex min-w-0 items-center gap-2 text-sm">
              <span class="text-muted-foreground hidden sm:inline">
                SQL Editor
              </span>
              <span class="text-muted-foreground hidden sm:inline">›</span>
              <span class="truncate font-semibold">{props.script.name}</span>
              <Show when={dirty()}>
                <span class="bg-warning/15 text-warning-foreground rounded-full px-2 py-0.5 text-xs">
                  Unsaved
                </span>
              </Show>
            </div>
          </div>

          <div class="flex min-w-0 items-center gap-2">
            <Select<string>
              multiple={true}
              options={[...(databases() ?? [])]}
              value={attachedDbs()}
              itemComponent={(props) => (
                <SelectItem item={props.item}>{props.item.rawValue}</SelectItem>
              )}
              onChange={(value: string[]) => setAttachedDbs(value)}
            >
              <SelectTrigger aria-label="Attached databases">
                <SelectValue class="max-w-40 min-w-16 text-ellipsis">
                  {(state) => {
                    const selected = state.selectedOptions();
                    return selected.length === 0
                      ? "No databases"
                      : selected.join(", ");
                  }}
                </SelectValue>
              </SelectTrigger>
              <SelectContent />
            </Select>

            <HelpDialog />
          </div>
        </div>
      </header>

      <div class="flex flex-col gap-3 p-3 sm:p-4">
        {(uiState().showMigrationWarning ?? true) && (
          <Callout class="flex items-start justify-between gap-3 text-sm">
            <p class="leading-relaxed">{migrationWarning}</p>
            <Button
              variant="ghost"
              size="icon"
              class="size-7 shrink-0"
              aria-label="Dismiss migration warning"
              onClick={() => {
                $uiState.set({
                  ...uiState(),
                  showMigrationWarning: false,
                });
              }}
            >
              <TbOutlineX />
            </Button>
          </Callout>
        )}

        <div
          class="min-h-64 overflow-hidden rounded-md border [&_.cm-editor]:min-h-64"
          ref={ref}
        />

        <QueryActionBar
          busy={executionResult.isFetching}
          mobile={isMobile()}
          onSave={saveScript}
          onExecute={execute}
        />
      </div>

      <div class="border-t">
        <ResultView
          script={props.script}
          response={executionResult.data ?? undefined}
          running={executionResult.isFetching}
        />
      </div>
    </Dialog>
  );
}

export function EditorPage() {
  // FIXME: Note that the state isn't persistent enough. E.g. resizing to
  // mobile rebuild EditorPage and reset the dirty state.
  const scripts = useStore($scripts);
  const isMobile = createIsMobile();
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
    <SidebarProvider
      cookieName="sql-explorer:state"
      style={{ "--sidebar-width": "15rem" }}
    >
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
              deleteScriptByIdx={deleteScriptByIdx}
            />
          </SidebarGroup>

          {/* <SidebarFooter /> */}
        </SidebarContent>

        <SidebarRail />
      </Sidebar>

      <SidebarInset>
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

export type Script = {
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

const migrationWarning =
  "\
When changing schemas, consider using migrations for \
cross-deployment consistency (dev, test, prod, etc.) One-off changes \
may lead to skew. Alterations using the table browser will yield migrations.";

// Needed for lazy load.
export default EditorPage;
