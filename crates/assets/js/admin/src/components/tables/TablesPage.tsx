import { For, Match, Show, Switch, createMemo, createSignal } from "solid-js";
import { useNavigate, useParams, type Navigator } from "@solidjs/router";
import { persistentAtom } from "@nanostores/persistent";
import { useStore } from "@nanostores/solid";

import { TablePane } from "@/components/tables/TablePane";
import { Button } from "@/components/ui/button";
import { SheetContent } from "@/components/ui/sheet";
import {
  TbTablePlus,
  TbTable,
  TbLock,
  TbLockOpen,
  TbEye,
  TbWand,
} from "solid-icons/tb";

import { CreateAlterTableForm } from "@/components/tables/CreateAlterTable";
import { SafeSheet } from "@/components/SafeSheet";
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
} from "@/components/ui/sidebar";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";

import { createTableSchemaQuery } from "@/lib/api/table";
import {
  hiddenTable,
  tableType,
  compareQualifiedNames,
  prettyFormatQualifiedName,
  equalQualifiedNames,
} from "@/lib/schema";
import { createIsMobile } from "@/lib/signals";

import type { ListSchemasResponse } from "@bindings/ListSchemasResponse";
import type { Table } from "@bindings/Table";
import type { View } from "@bindings/View";
import { QualifiedName } from "@bindings/QualifiedName";

function pickInitiallySelectedTable(
  tables: (Table | View)[],
  qualifiedTableName: string | undefined,
): Table | View | undefined {
  if (tables.length === 0) {
    return undefined;
  }

  const candidate = qualifiedTableName ?? $explorerSettings.get().prevSelected;
  if (candidate) {
    for (const table of tables) {
      if (candidate === prettyFormatQualifiedName(table.name)) {
        return table;
      }
    }
  }

  const first = tables[0];
  console.debug(
    `Table '${qualifiedTableName}' not found. Fallback: ${prettyFormatQualifiedName(first.name)}`,
  );
  return first;
}

function tableCompare(a: Table | View, b: Table | View): number {
  const aHidden = hiddenTable(a);
  const bHidden = hiddenTable(b);

  if (aHidden == bHidden) {
    return compareQualifiedNames(a.name, b.name);
  }
  // Sort hidden tables to the back.
  return aHidden ? 1 : -1;
}

function TablePickerSidebar(props: {
  tablesAndViews: (Table | View)[];
  allTables: Table[];
  selectedTable: Table | View | undefined;
  schemaRefetch: () => Promise<void>;
  openCreateTableDialog: () => void;
}) {
  const { setOpenMobile } = useSidebar();
  const settings = useStore($explorerSettings);
  const showHidden = () => settings().showHidden ?? false;
  const selectedTable = () => props.selectedTable;
  const navigate = useNavigate();

  return (
    <div class="p-2">
      <SidebarGroupContent>
        <SidebarMenu>
          {/* Add table & show hidden tables buttons */}
          <div class="flex w-full justify-between gap-2">
            <Button
              class="min-w-[100px] grow gap-2"
              variant="secondary"
              onClick={() => {
                setOpenMobile(false);
                props.openCreateTableDialog();
              }}
            >
              <TbTablePlus />
              Add Table
            </Button>

            <Tooltip>
              <TooltipTrigger as="div">
                <Button
                  size="icon"
                  variant="secondary"
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
                  <Show when={showHidden()} fallback={<TbLock />}>
                    <TbLockOpen />
                  </Show>
                </Button>
              </TooltipTrigger>

              <TooltipContent>
                Toggle visibility of hidden tables.
              </TooltipContent>
            </Tooltip>
          </div>

          <For each={props.tablesAndViews}>
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
                        <TbEye />
                      </Match>

                      <Match when={type === "virtualTable"}>
                        <TbWand />
                      </Match>

                      <Match when={type === "table"}>
                        <TbTable />
                      </Match>
                    </Switch>

                    <span class="truncate">{name}</span>
                    {hidden && <TbLock />}
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
  const filteredTablesAndViews = createMemo(() => {
    const all = [...props.schemas.tables, ...props.schemas.views];

    const show = showHidden();
    if (show) {
      return all.sort(tableCompare);
    }
    return all.filter((t) => !hiddenTable(t)).sort(tableCompare);
  });

  const params = useParams<{ table: string }>();
  const selectedTable = createMemo(() => {
    const allTables = filteredTablesAndViews();
    // useParams returns undefined as a string.
    const table = params.table === "undefined" ? undefined : params.table;
    return pickInitiallySelectedTable(allTables, table);
  });

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
                allTables={props.schemas.tables}
                setSelected={(tableName: QualifiedName) => {
                  const table = filteredTablesAndViews().find((t) =>
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
                <SidebarContent>
                  {/* <SidebarHeader /> */}

                  <SidebarGroup>
                    <TablePickerSidebar
                      tablesAndViews={filteredTablesAndViews()}
                      allTables={props.schemas.tables}
                      selectedTable={selectedTable()}
                      schemaRefetch={props.schemaRefetch}
                      openCreateTableDialog={() => setCreateTableDialog(true)}
                    />
                  </SidebarGroup>

                  {/* <SidebarFooter /> */}
                </SidebarContent>

                <SidebarRail />
              </Sidebar>

              <SidebarInset class="min-w-0">
                <Show
                  when={selectedTable() !== undefined}
                  fallback={<div class="p-4">No table selected</div>}
                >
                  <Switch>
                    <Match when={isMobile()}>
                      <TablePane
                        selectedTable={selectedTable()!}
                        schemas={props.schemas}
                        schemaRefetch={props.schemaRefetch}
                      />
                    </Match>

                    <Match when={!isMobile()}>
                      <div class="h-dvh overflow-y-auto">
                        <TablePane
                          selectedTable={selectedTable()!}
                          schemas={props.schemas}
                          schemaRefetch={props.schemaRefetch}
                        />
                      </div>
                    </Match>
                  </Switch>
                </Show>
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
  prevSelected?: String;
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
