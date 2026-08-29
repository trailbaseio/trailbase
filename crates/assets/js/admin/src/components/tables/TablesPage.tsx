import { For, Match, Show, Switch, createMemo, createSignal } from "solid-js";
import { useNavigate, useParams, type Navigator } from "@solidjs/router";
import { persistentAtom } from "@nanostores/persistent";
import { useStore } from "@nanostores/solid";

import { TablePane } from "@/components/tables/TablePane";
import { Button } from "@/components/ui/button";
import { SheetContent } from "@/components/ui/sheet";
import {
  TbOutlineEye,
  TbOutlineLock,
  TbOutlineLockOpen,
  TbOutlineTable,
  TbOutlineTablePlus,
  TbOutlineWand,
} from "solid-icons/tb";

import { CreateAlterTableForm } from "@/components/tables/CreateAlterTable";
import { SafeSheet } from "@/components/SafeSheet";
import {
  useSidebar,
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarInput,
  SidebarInset,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarRail,
} from "@/components/ui/sidebar";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";

import { createTableSchemaQuery } from "@/lib/api/table";
import { createSystemInfoQuery } from "@/lib/api/info";
import {
  hiddenTable,
  tableType,
  prettyFormatQualifiedName,
  equalQualifiedNames,
} from "@/lib/schema";
import { createIsMobile } from "@/lib/signals";

import type { ListSchemasResponse } from "@bindings/ListSchemasResponse";
import type { Table } from "@bindings/Table";
import type { View } from "@bindings/View";
import { QualifiedName } from "@bindings/QualifiedName";

export function resourceSchemaName(resource: Table | View): string {
  return resource.name.database_schema || "main";
}

export function filterExplorerResources<T extends Table | View>(
  resources: T[],
  query: string,
): T[] {
  const normalized = query.trim().toLocaleLowerCase();
  if (!normalized) return resources;

  return resources.filter((resource) =>
    prettyFormatQualifiedName(resource.name)
      .toLocaleLowerCase()
      .includes(normalized),
  );
}

export function groupExplorerResources<T extends Table | View>(
  resources: T[],
): [string, T[]][] {
  const groups = new Map<string, T[]>();
  for (const resource of resources) {
    const schema = resourceSchemaName(resource);
    const group = groups.get(schema);
    if (group) {
      group.push(resource);
    } else {
      groups.set(schema, [resource]);
    }
  }
  return [...groups];
}

function pickInitiallySelectedTable(
  tables: ([Table, string] | [View, string])[],
  qualifiedTableName: string | undefined,
): [Table, string] | [View, string] | undefined {
  if (tables.length === 0) {
    return undefined;
  }

  const candidate = qualifiedTableName ?? $explorerSettings.get().prevSelected;

  if (candidate) {
    for (const table of tables) {
      if (candidate === prettyFormatQualifiedName(table[0].name)) {
        return table;
      }
    }
  }

  const first = tables[0];
  console.debug(
    `Table '${qualifiedTableName}' not found. Fallback: ${prettyFormatQualifiedName(first[0].name)}`,
  );
  return first;
}

function tableCompare(
  a: [Table, string] | [View, string],
  b: [Table, string] | [View, string],
): number {
  const aHidden = hiddenTable(a[0]);
  const bHidden = hiddenTable(b[0]);

  if (aHidden == bHidden) {
    return prettyFormatQualifiedName(a[0].name).localeCompare(
      prettyFormatQualifiedName(b[0].name),
    );
  }
  // Sort hidden tables to the back.
  return aHidden ? 1 : -1;
}

function TablePickerSidebar(props: {
  tablesAndViews: (Table | View)[];
  selectedTable: Table | View | undefined;
  openCreateTableDialog: () => void;
  postgres: boolean;
}) {
  const { setOpenMobile } = useSidebar();
  const settings = useStore($explorerSettings);
  const showHidden = () => settings().showHidden ?? false;
  const selectedTable = () => props.selectedTable;
  const navigate = useNavigate();
  const [search, setSearch] = createSignal("");
  const filteredResources = createMemo(() =>
    filterExplorerResources(props.tablesAndViews, search()),
  );
  const groupedResources = createMemo(() =>
    groupExplorerResources(filteredResources()),
  );

  const toggleHiddenResources = () => {
    const nextShowHidden = !showHidden();
    const current = selectedTable();

    if (!nextShowHidden && current && hiddenTable(current)) {
      navigateToTable(navigate, undefined);
    }

    $explorerSettings.set({
      ...$explorerSettings.get(),
      showHidden: nextShowHidden,
    });
  };

  return (
    <>
      <SidebarHeader class="border-sidebar-border gap-3 border-b p-3">
        <div class="flex items-center gap-2">
          <div class="min-w-0 flex-1">
            <h2 class="text-sm font-semibold">Tables</h2>
            <p
              class="text-muted-foreground text-xs tabular-nums"
              aria-live="polite"
            >
              {filteredResources().length} visible
            </p>
          </div>

          <Tooltip>
            <TooltipTrigger as="div">
              <Button
                size="icon"
                variant="ghost"
                aria-label="Add table"
                disabled={props.postgres}
                onClick={() => {
                  setOpenMobile(false);
                  props.openCreateTableDialog();
                }}
              >
                <TbOutlineTablePlus />
              </Button>
            </TooltipTrigger>
            <TooltipContent>
              {props.postgres
                ? "Table creation is unavailable for PostgreSQL"
                : "Add table"}
            </TooltipContent>
          </Tooltip>
        </div>

        <SidebarInput
          type="search"
          aria-label="Search tables and views"
          placeholder="Search tables and views"
          value={search()}
          onInput={(event) => setSearch(event.currentTarget.value)}
        />
      </SidebarHeader>

      <SidebarContent class="py-2">
        <Show
          when={filteredResources().length > 0}
          fallback={
            <div class="text-muted-foreground flex flex-col items-start gap-2 px-4 py-6 text-sm">
              <p>
                {search().trim()
                  ? "No tables or views match your search."
                  : "No tables or views available."}
              </p>
              <Show when={search().trim()}>
                <Button
                  class="h-auto p-0"
                  variant="link"
                  onClick={() => setSearch("")}
                >
                  Clear search
                </Button>
              </Show>
            </div>
          }
        >
          <For each={groupedResources()}>
            {([schema, resources]) => (
              <SidebarGroup class="px-2 py-1">
                <SidebarGroupLabel class="px-2">
                  <span class="truncate" title={schema}>
                    {schema}
                  </span>
                  <span class="ml-auto tabular-nums">{resources.length}</span>
                </SidebarGroupLabel>
                <SidebarGroupContent>
                  <SidebarMenu>
                    <For each={resources}>
                      {(item) => {
                        const hidden = hiddenTable(item);
                        const type = tableType(item);
                        const selected = () => {
                          const current = selectedTable();
                          return (
                            current !== undefined &&
                            equalQualifiedNames(item.name, current.name)
                          );
                        };
                        const name = item.name.name;
                        const qualifiedName = prettyFormatQualifiedName(
                          item.name,
                        );

                        return (
                          <SidebarMenuItem>
                            <Tooltip placement="right">
                              <TooltipTrigger as="div" class="w-full">
                                <SidebarMenuButton
                                  class={
                                    hidden ? "text-muted-foreground" : undefined
                                  }
                                  isActive={selected()}
                                  variant="default"
                                  size="default"
                                  onClick={() => {
                                    setOpenMobile(false);
                                    navigateToTable(navigate, item);
                                  }}
                                >
                                  <Switch>
                                    <Match when={type === "view"}>
                                      <TbOutlineEye />
                                    </Match>
                                    <Match when={type === "virtualTable"}>
                                      <TbOutlineWand />
                                    </Match>
                                    <Match when={type === "table"}>
                                      <TbOutlineTable />
                                    </Match>
                                  </Switch>
                                  <span class="min-w-0 flex-1 truncate">
                                    {name}
                                  </span>
                                  <Show when={hidden}>
                                    <TbOutlineLock class="ml-auto" />
                                  </Show>
                                </SidebarMenuButton>
                              </TooltipTrigger>
                              <TooltipContent>{qualifiedName}</TooltipContent>
                            </Tooltip>
                          </SidebarMenuItem>
                        );
                      }}
                    </For>
                  </SidebarMenu>
                </SidebarGroupContent>
              </SidebarGroup>
            )}
          </For>
        </Show>
      </SidebarContent>

      <SidebarFooter class="border-sidebar-border border-t p-2">
        <Tooltip>
          <TooltipTrigger as="div" class="w-full">
            <Button
              class="w-full justify-start"
              variant={showHidden() ? "secondary" : "ghost"}
              aria-label={
                showHidden()
                  ? "Hide hidden tables and views"
                  : "Show hidden tables and views"
              }
              aria-pressed={showHidden()}
              onClick={toggleHiddenResources}
            >
              <Show when={showHidden()} fallback={<TbOutlineLock />}>
                <TbOutlineLockOpen />
              </Show>
              {showHidden() ? "Hide hidden resources" : "Show hidden resources"}
            </Button>
          </TooltipTrigger>
          <TooltipContent>
            {showHidden()
              ? "Hide internal tables and views"
              : "Show internal tables and views"}
          </TooltipContent>
        </Tooltip>
      </SidebarFooter>
    </>
  );
}

function navigateToTable(navigate: Navigator, table: Table | View | undefined) {
  const name =
    table !== undefined ? prettyFormatQualifiedName(table.name) : undefined;

  $explorerSettings.set({
    ...$explorerSettings.get(),
    prevSelected: name,
  });

  const path = `/table/${name ?? ""}`;
  console.debug(`navigating to: ${path}`);
  navigate(path);
}

function TableSplitView(props: {
  schemas: ListSchemasResponse;
  schemaRefetch: () => Promise<void>;
}) {
  const navigate = useNavigate();
  const isMobile = createIsMobile();
  const settings = useStore($explorerSettings);
  const showHidden = () => settings().showHidden ?? false;
  const [createTableDialog, setCreateTableDialog] = createSignal(false);

  const allTables = createMemo(() => props.schemas.tables.map(([t, _]) => t));
  const filteredTablesAndViews = createMemo(() => {
    const all = [...props.schemas.tables, ...props.schemas.views];

    const show = showHidden();
    if (show) {
      return all.sort(tableCompare);
    }
    return all.filter(([t, _]) => !hiddenTable(t)).sort(tableCompare);
  });

  const params = useParams<{ table: string | undefined }>();
  const selectedTable = createMemo(() => {
    const filteredTables = filteredTablesAndViews();

    // NOTE: useParams used to return undefined as a "undefined" string. Does no longer seem to be the case.
    // We can probably simplify this.
    const table: string | undefined =
      params.table === undefined || params.table === "undefined"
        ? undefined
        : decodeURIComponent(params.table);

    return pickInitiallySelectedTable(filteredTables, table);
  });

  const systemInfo = createSystemInfoQuery();
  const isPostgres = () => systemInfo.data?.postgres ?? false;

  return (
    <SafeSheet
      id="add_table_dialog"
      open={[createTableDialog, setCreateTableDialog]}
    >
      {(sheet) => {
        return (
          <>
            <SheetContent class="sm:max-w-[520px]">
              <CreateAlterTableForm
                schemaRefetch={props.schemaRefetch}
                allTables={allTables()}
                setSelected={(tableName: QualifiedName) => {
                  const table = filteredTablesAndViews().find(([t, _]) =>
                    equalQualifiedNames(t.name, tableName),
                  );
                  if (table) {
                    navigateToTable(navigate, table[0]);
                  }
                }}
                {...sheet}
              />
            </SheetContent>

            <SidebarProvider
              cookieName="table-explorer:state"
              style={{ "--sidebar-width": "16rem" }}
            >
              <Sidebar
                class="absolute"
                variant="sidebar"
                side="left"
                collapsible="offcanvas"
              >
                <TablePickerSidebar
                  tablesAndViews={filteredTablesAndViews().map(([t, _]) => t)}
                  selectedTable={selectedTable()?.[0]}
                  openCreateTableDialog={() => setCreateTableDialog(true)}
                  postgres={isPostgres()}
                />

                <SidebarRail />
              </Sidebar>

              <SidebarInset>
                <Switch>
                  <Match when={selectedTable() !== undefined && isMobile()}>
                    <TablePane
                      selectedTable={selectedTable()!}
                      schemas={props.schemas}
                      schemaRefetch={props.schemaRefetch}
                      postgres={isPostgres()}
                    />
                  </Match>

                  <Match when={selectedTable() !== undefined && !isMobile()}>
                    <div class="h-dvh overflow-y-auto">
                      <TablePane
                        selectedTable={selectedTable()!}
                        schemas={props.schemas}
                        schemaRefetch={props.schemaRefetch}
                        postgres={isPostgres()}
                      />
                    </div>
                  </Match>

                  <Match when={true}>
                    <div class="p-4">No table selected</div>
                  </Match>
                </Switch>
              </SidebarInset>
            </SidebarProvider>
          </>
        );
      }}
    </SafeSheet>
  );
}

export function TablePage() {
  const schemaFetch = createTableSchemaQuery();
  const schemaRefetch = async () => {
    const schemas = await schemaFetch.refetch();
    console.debug("All table schemas re-fetched:", schemas);
  };

  return (
    <Switch>
      <Match when={schemaFetch.isError}>
        <span>Schema fetch error: {JSON.stringify(schemaFetch.error)}</span>
      </Match>

      <Match when={schemaFetch.data}>
        <TableSplitView
          schemas={schemaFetch.data!}
          schemaRefetch={schemaRefetch}
        />
      </Match>
    </Switch>
  );
}

type Settings = {
  prevSelected?: string;
  showHidden?: boolean;
};

const $explorerSettings = persistentAtom<Settings>(
  "explorer_settings",
  {},
  {
    encode: JSON.stringify,
    decode: JSON.parse,
  },
);
