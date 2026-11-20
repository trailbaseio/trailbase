import { For, Match, Show, Switch, createMemo, createSignal } from "solid-js";
import { useNavigate, useParams, type Navigator } from "@solidjs/router";
import { persistentJSON } from "@nanostores/persistent";
import { useStore } from "@nanostores/solid";
import {
  TbOutlineLock,
  TbOutlineLockOpen,
  TbOutlineTablePlus,
} from "solid-icons/tb";

import { TablePane } from "@/components/explorer/TablePane";
import { SchemaIcon } from "@/components/explorer/SchemaIcon";
import { CreateAlterTableForm } from "@/components/explorer/CreateAlterTable";
import { SafeSheet } from "@/components/SafeSheet";

import { Button } from "@/components/ui/button";
import { SheetContent } from "@/components/ui/sheet";
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
import { TextField, TextFieldInput } from "@/components/ui/text-field";
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

import type {
  ListSchemasResponse,
  QualifiedName,
  Schema,
  Table,
  View,
} from "trailbase-bindings";

function pickInitiallySelectedTable(
  tables: (Schema<Table> | Schema<View>)[],
  qualifiedTableName: string | undefined,
): Schema<Table> | Schema<View> | undefined {
  if (tables.length === 0) {
    return undefined;
  }

  const candidate = qualifiedTableName ?? $explorerSettings.get().prevSelected;

  if (candidate) {
    for (const table of tables) {
      if (candidate === prettyFormatQualifiedName(table.schema.name)) {
        return table;
      }
    }
  }

  const first = tables.find((t) => !hiddenTable(t.schema)) ?? tables[0];
  console.debug(
    `Table '${qualifiedTableName}' not found. Fallback: ${prettyFormatQualifiedName(first.schema.name)}`,
  );
  return first;
}

function tableSorter(
  a: Schema<Table> | Schema<View>,
  b: Schema<Table> | Schema<View>,
): number {
  const [aDb, bDb] = [schemaTenantKey(a), schemaTenantKey(b)];
  if (aDb !== bDb) {
    return aDb.localeCompare(bDb);
  }

  const [aHidden, bHidden] = [hiddenTable(a.schema), hiddenTable(b.schema)];
  if (aHidden == bHidden) {
    return a.schema.name.name.localeCompare(b.schema.name.name);
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

function schemaTenantKey(tableOrView: Schema<Table> | Schema<View>): string {
  const tenant = tableOrView.tenant;
  if (tenant) {
    return `${tenant.template}::${tenant.name ?? tenant.id ?? "golden"}`;
  }
  return tableOrView.schema.name.database_schema ?? "main";
}

function TablePickerSidebar(props: {
  tablesAndViews: (Schema<Table> | Schema<View>)[];
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

  const [search, setSearch] = createSignal("");

  const tablesAndViewsBySchema = createMemo(
    (): (Schema<Table> | Schema<View>)[][] => {
      const show = showHidden();
      const filtered = props.tablesAndViews.filter((t) => {
        if (search() !== "") {
          return t.schema.name.name.search(search()) !== -1;
        }

        return show || !hiddenTable(t.schema) || t.schema === selectedTable();
      });

      const bySchema = groupBy(filtered, schemaTenantKey);

      // Sort by name and visibility.
      for (const tables of bySchema) {
        tables.sort(tableSorter);
      }

      return bySchema;
    },
  );

  const count = () =>
    tablesAndViewsBySchema().reduce((c, s) => c + s.length, 0);

  return (
    <>
      {/* Add table & show hidden tables buttons */}
      <SidebarHeader>
        <div class="flex justify-between gap-2 p-2">
          <div>
            <h3>Schemas</h3>
            <span class="text-xs">{count()} visible</span>
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

        <TextField>
          <TextFieldInput
            type="text"
            placeholder="search"
            onInput={(e: InputEvent) => {
              setSearch((e.target as HTMLInputElement).value);
            }}
          />
        </TextField>
      </SidebarHeader>

      <SidebarContent class="px-2">
        <For each={tablesAndViewsBySchema()}>
          {(tables: (Schema<Table> | Schema<View>)[]) => (
            <SidebarGroup>
              <SidebarGroupLabel>
                <div class="flex w-full justify-between gap-2">
                  <span>{schemaTenantKey(tables[0])}</span>

                  <span class="text-xs">{tables.length}</span>
                </div>
              </SidebarGroupLabel>

              <SidebarGroupContent>
                <SidebarMenu>
                  <For each={tables}>
                    {(item: Schema<Table> | Schema<View>) => {
                      const hidden = hiddenTable(item.schema);
                      const type = tableType(item.schema);
                      const selected = () => {
                        const s = selectedTable();
                        if (s !== undefined) {
                          return equalQualifiedNames(item.schema.name, s.name);
                        }
                        return false;
                      };

                      const name = prettyFormatQualifiedName(item.schema.name);

                      return (
                        <SidebarMenuItem>
                          <SidebarMenuButton
                            isActive={selected()}
                            tooltip={prettyFormatQualifiedName(
                              item.schema.name,
                            )}
                            variant="default"
                            size="md"
                            onClick={() => {
                              setOpenMobile(false);
                              navigateToTable(navigate, item.schema);
                            }}
                          >
                            <SchemaIcon type={type} />

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
  const [createTableDialog, setCreateTableDialog] = createSignal(false);

  const allTables = createMemo(() => props.schemas.tables.map((t) => t.schema));
  const allTablesAndView = createMemo(() => [
    ...props.schemas.tables,
    ...props.schemas.views,
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
                    equalQualifiedNames(t.schema.name, tableName),
                  );

                  if (table) {
                    navigateToTable(navigate, table.schema);
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
                  selectedTable={selectedTable()?.schema}
                  schemaRefetch={props.schemaRefetch}
                  openCreateTableDialog={() => setCreateTableDialog(true)}
                  postgres={isPostgres()}
                />

                <SidebarRail />
              </Sidebar>

              <SidebarInset>
                <Switch>
                  <Match when={selectedTable()}>
                    <TablePane
                      selectedTable={selectedTable()!}
                      schemas={props.schemas}
                      schemaRefetch={props.schemaRefetch}
                      postgres={isPostgres()}
                    />
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
    <div class="size-full scrollbar-thin md:overflow-y-auto">
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
    </div>
  );
}

type Settings = {
  prevSelected?: string;
  showHidden?: boolean;
};

const $explorerSettings = persistentJSON<Settings>("explorer_settings", {});
