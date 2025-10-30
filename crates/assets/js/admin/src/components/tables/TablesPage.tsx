import { For, Match, Show, Switch, createMemo } from "solid-js";
import { useNavigate, useParams, type Navigator } from "@solidjs/router";
import { persistentAtom } from "@nanostores/persistent";
import { useStore } from "@nanostores/solid";
import type { DialogTriggerProps } from "@kobalte/core/dialog";

import { TablePane } from "@/components/tables/TablePane";
import { Button } from "@/components/ui/button";
import { SheetContent, SheetTrigger } from "@/components/ui/sheet";
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

  if (qualifiedTableName) {
    for (const table of tables) {
      if (qualifiedTableName === prettyFormatQualifiedName(table.name)) {
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
}) {
  const showHidden = useStore($showHiddenTables);
  const selectedTable = () => props.selectedTable;
  const navigate = useNavigate();

  return (
    <div class={`hide-scrollbars flex flex-col gap-2 overflow-scroll p-2`}>
      <div class="flex w-full justify-between gap-2">
        <SafeSheet>
          {(sheet) => {
            return (
              <>
                <SheetContent class={sheetMaxWidth}>
                  <CreateAlterTableForm
                    schemaRefetch={props.schemaRefetch}
                    allTables={props.allTables}
                    setSelected={(tableName: QualifiedName) => {
                      const table = props.tablesAndViews.find((t) =>
                        equalQualifiedNames(t.name, tableName),
                      );
                      if (table) {
                        navigateToTable(navigate, table);
                      }
                    }}
                    {...sheet}
                  />
                </SheetContent>

                <SheetTrigger
                  as={(props: DialogTriggerProps) => (
                    <Button
                      class="min-w-[100px] grow gap-2"
                      variant="secondary"
                      {...props}
                    >
                      <TbTablePlus />
                      Add Table
                    </Button>
                  )}
                />
              </>
            );
          }}
        </SafeSheet>

        <Tooltip>
          <TooltipTrigger as="div">
            <Button
              size="icon"
              variant="secondary"
              onClick={() => {
                const show = !showHidden();
                const current = selectedTable();
                if (!show && current && hiddenTable(current)) {
                  navigateToTable(navigate, undefined);
                }
                console.debug("Show hidden tables:", show);
                $showHiddenTables.set(show);
              }}
            >
              <Show when={showHidden()} fallback={<TbLock />}>
                <TbLockOpen />
              </Show>
            </Button>
          </TooltipTrigger>

          <TooltipContent>Toggle visibility of hidden tables.</TooltipContent>
        </Tooltip>
      </div>

      <SidebarGroupContent>
        <SidebarMenu>
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
                    onClick={() => navigateToTable(navigate, item)}
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
  if (table === undefined) {
    navigate("/table/");
    return;
  }

  const path = "/table/" + prettyFormatQualifiedName(table.name);
  console.debug(`navigating to: ${path}`);
  navigate(path);
}

function TableSplitView(props: {
  schemas: ListSchemasResponse;
  schemaRefetch: () => Promise<void>;
}) {
  const isMobile = createIsMobile();
  const showHidden = useStore($showHiddenTables);
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

const sheetMaxWidth = "sm:max-w-[520px]";
const $showHiddenTables = persistentAtom<boolean>("show_hidden_tables", false, {
  encode: JSON.stringify,
  decode: JSON.parse,
});
