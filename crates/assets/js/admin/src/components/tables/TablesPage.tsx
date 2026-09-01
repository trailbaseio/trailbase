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
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
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

  const first = tables.find(([t, _]) => !hiddenTable(t)) ?? tables[0];
  console.debug(
    `Table '${qualifiedTableName}' not found. Fallback: ${prettyFormatQualifiedName(first[0].name)}`,
  );
  return first;
}

function tableCompare(a: Table | View, b: Table | View): number {
  const [aDb, bDb] = [
    a.name.database_schema ?? "main",
    b.name.database_schema ?? "main",
  ];
  if (aDb !== bDb) {
    return aDb.localeCompare(bDb);
  }

  const [aHidden, bHidden] = [hiddenTable(a), hiddenTable(b)];
  if (aHidden == bHidden) {
    return a.name.name.localeCompare(b.name.name);
  }
  // Sort hidden tables to the back.
  return aHidden ? 1 : -1;
}

function groupBy<T, K>(arr: T[], keySelector: (item: T) => K): T[][] {
  const groups = arr.reduce((acc, item) => {
    const key = keySelector(item);
    if (!acc.has(key)) {
      acc.set(key, []);
    }
    acc.get(key)!.push(item);
    return acc;
  }, new Map<K, T[]>());

  return Array.from(groups.values());
}

function TablePickerSidebar(props: {
  tablesAndViews: (Table | View)[];
  allTables: Table[];
  selectedTable: Table | View | undefined;
  schemaRefetch: () => Promise<void>;
  openCreateTableDialog: () => void;
  postgres: boolean;
}) {
  const { setOpenMobile } = useSidebar();
  const settings = useStore($explorerSettings);
  const showHidden = () => settings().showHidden ?? false;
  const selectedTable = () => props.selectedTable;
  const navigate = useNavigate();

  const tablesAndViewsBySchema = createMemo((): (Table | View)[][] => {
    const show = showHidden();
    const bySchema = groupBy(
      show
        ? props.tablesAndViews
        : props.tablesAndViews.filter(
            (t) => !hiddenTable(t) || t === selectedTable(),
          ),
      (table) => table.name.database_schema ?? "main",
    );
    for (const tables of bySchema) {
      tables.sort(tableCompare);
    }
    return bySchema;
  });

  return (
    <>
      {/* Add table & show hidden tables buttons */}
      <SidebarHeader>
        <div class="flex justify-between gap-2 p-2">
          <div>
            <h3>Schemas</h3>
            <span class="text-xs">{props.tablesAndViews.length} visible</span>
          </div>

          <div class="flex gap-1">
            <Tooltip>
              <TooltipTrigger as="div">
                <Button
                  size="icon"
                  variant="ghost"
                  onClick={() => {
                    const nextShowHidden = !(settings().showHidden ?? false);
                    const currentHidden = () => {
                      const current = selectedTable();
                      if (current !== undefined) {
                        return hiddenTable(current);
                      }
                      return false;
                    };

                    if (!nextShowHidden && currentHidden()) {
                      navigateToTable(navigate, undefined);
                    }

                    $explorerSettings.set({
                      ...$explorerSettings.get(),
                      showHidden: nextShowHidden,
                    });
                  }}
                >
                  <Show when={showHidden()} fallback={<TbOutlineLock />}>
                    <TbOutlineLockOpen />
                  </Show>
                </Button>
              </TooltipTrigger>

              <TooltipContent>
                Toggle visibility of hidden tables.
              </TooltipContent>
            </Tooltip>

            <Tooltip>
              <TooltipTrigger as="div">
                <Button
                  variant="ghost"
                  size="icon"
                  disabled={props.postgres}
                  onClick={() => {
                    setOpenMobile(false);
                    props.openCreateTableDialog();
                  }}
                >
                  <TbOutlineTablePlus />
                </Button>
              </TooltipTrigger>

              <TooltipContent>Add new table.</TooltipContent>
            </Tooltip>
          </div>
        </div>
      </SidebarHeader>

      <SidebarContent class="px-2">
        <For each={tablesAndViewsBySchema()}>
          {(tables: (Table | View)[]) => (
            <SidebarGroup>
              <SidebarGroupLabel>
                <div class="flex w-full justify-between gap-2">
                  <span>{tables[0].name.database_schema ?? "main"}</span>

                  <span class="text-xs">{tables.length}</span>
                </div>
              </SidebarGroupLabel>

              <SidebarGroupContent>
                <SidebarMenu>
                  <For each={tables}>
                    {(item: Table | View) => {
                      const hidden = hiddenTable(item);
                      const type = tableType(item);
                      const selected = () => {
                        const s = selectedTable();
                        if (s !== undefined) {
                          return equalQualifiedNames(item.name, s.name);
                        }
                        return false;
                      };

                      const name = prettyFormatQualifiedName(item.name);

                      return (
                        <SidebarMenuItem>
                          <SidebarMenuButton
                            isActive={selected()}
                            tooltip={prettyFormatQualifiedName(item.name)}
                            variant="default"
                            size="md"
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

                            <span class="truncate">{name}</span>
                            {hidden && <TbOutlineLock />}
                          </SidebarMenuButton>
                        </SidebarMenuItem>
                      );
                    }}
                  </For>
                </SidebarMenu>
              </SidebarGroupContent>
            </SidebarGroup>
          )}
        </For>
      </SidebarContent>
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
  const [createTableDialog, setCreateTableDialog] = createSignal(false);

  const allTables = createMemo(() => props.schemas.tables.map(([t, _]) => t));
  const allTablesAndView = createMemo(() => [
    ...props.schemas.tables.map(([t, _]) => t),
    ...props.schemas.views.map(([v, _]) => v),
  ]);

  const params = useParams<{ table: string | undefined }>();
  const selectedTable = createMemo(() => {
    const all = [...props.schemas.tables, ...props.schemas.views];

    // NOTE: useParams used to return undefined as a "undefined" string. Does no longer seem to be the case.
    // We can probably simplify this.
    const table: string | undefined =
      params.table === undefined || params.table === "undefined"
        ? undefined
        : decodeURIComponent(params.table);

    return pickInitiallySelectedTable(all, table);
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
                  const table = allTablesAndView().find((t) =>
                    equalQualifiedNames(t.name, tableName),
                  );
                  if (table) {
                    navigateToTable(navigate, table);
                  }
                }}
                {...sheet}
              />
            </SheetContent>

            <SidebarProvider>
              <Sidebar
                class="absolute"
                variant="sidebar"
                side="left"
                collapsible="offcanvas"
              >
                <TablePickerSidebar
                  tablesAndViews={allTablesAndView()}
                  allTables={allTables()}
                  selectedTable={selectedTable()?.[0]}
                  schemaRefetch={props.schemaRefetch}
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
