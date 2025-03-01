import { For, Match, Show, Switch, createMemo, createResource } from "solid-js";
import { useNavigate, useParams, type Navigator } from "@solidjs/router";
import { persistentAtom } from "@nanostores/persistent";
import { useStore } from "@nanostores/solid";
import type { DialogTriggerProps } from "@kobalte/core/dialog";

import { TablePane } from "@/components/tables/TablePane";
import { Button } from "@/components/ui/button";
import { SheetContent, SheetTrigger } from "@/components/ui/sheet";
import {
  Switch as SwitchUi,
  SwitchControl,
  SwitchLabel,
  SwitchThumb,
} from "@/components/ui/switch";
import { TbTablePlus, TbLock, TbEye, TbWand } from "solid-icons/tb";

import { CreateAlterTableForm } from "@/components/tables/CreateAlterTable";
import { SplitView } from "@/components/SplitView";
import { SafeSheet } from "@/components/SafeSheet";
import { Separator } from "@/components/ui/separator";

import { getAllTableSchemas } from "@/lib/table";

import type { ListSchemasResponse, Table, View } from "@/lib/bindings";
import { hiddenTable, tableType } from "@/lib/schema";

function pickInitiallySelectedTable(
  tables: (Table | View)[],
  tableName: string | undefined,
): Table | View | undefined {
  if (tables.length === 0) {
    return undefined;
  }

  if (tableName) {
    for (const table of tables) {
      if (tableName === table.name) {
        return table;
      }
    }
  }

  return tables[0];
}

function tableCompare(a: Table | View, b: Table | View): number {
  const aHidden = hiddenTable(a);
  const bHidden = hiddenTable(b);

  if (aHidden == bHidden) {
    return a.name.localeCompare(b.name);
  }
  // Sort hidden tables to the back.
  return aHidden ? 1 : -1;
}

function TablePickerPane(props: {
  horizontal: boolean;
  tablesAndViews: (Table | View)[];
  allTables: Table[];
  selectedTable: Table | View | undefined;
  schemaRefetch: () => Promise<void>;
}) {
  const showHidden = useStore($showHiddenTables);
  const selectedTable = () => props.selectedTable;
  const horizontal = () => props.horizontal;
  const navigate = useNavigate();

  return (
    <div
      class={`${horizontal() ? "flex h-dvh flex-col" : "flex"} hide-scrollbars gap-2 overflow-scroll p-4`}
    >
      <SwitchUi
        class="flex items-center justify-center gap-2"
        checked={showHidden()}
        onChange={(show: boolean) => {
          const current = selectedTable();
          if (!show && current && hiddenTable(current)) {
            navigateToTable(navigate, undefined);
          }
          console.debug("Show hidden tables:", show);
          $showHiddenTables.set(show);
        }}
      >
        <SwitchControl>
          <SwitchThumb />
        </SwitchControl>

        <SwitchLabel>Show Hidden</SwitchLabel>
      </SwitchUi>

      {horizontal() && <Separator />}

      <SafeSheet>
        {(sheet) => {
          return (
            <>
              <SheetContent class={sheetMaxWidth}>
                <CreateAlterTableForm
                  schemaRefetch={props.schemaRefetch}
                  allTables={props.allTables}
                  setSelected={(tableName: string) => {
                    const table = props.tablesAndViews.find(
                      (t) => t.name === tableName,
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
                    class="min-w-[100px] gap-2"
                    variant="secondary"
                    {...props}
                  >
                    {horizontal() && <TbTablePlus size={16} />}
                    Add Table
                  </Button>
                )}
              />
            </>
          );
        }}
      </SafeSheet>

      <For each={props.tablesAndViews}>
        {(item: Table | View) => {
          const hidden = hiddenTable(item);
          const type = tableType(item);
          const selected = () => item.name === selectedTable()?.name;

          return (
            <Button
              variant={selected() ? "default" : "outline"}
              class="flex gap-2"
              onClick={() => navigateToTable(navigate, item)}
            >
              <span
                class={
                  !selected() && hidden ? "truncate text-gray-500" : "truncate"
                }
              >
                {item.name}
              </span>
              {hidden && <TbLock />}
              {type === "view" && <TbEye />}
              {type === "virtualTable" && <TbWand />}
            </Button>
          );
        }}
      </For>
    </div>
  );
}

function navigateToTable(navigate: Navigator, table: Table | View | undefined) {
  navigate("/table/" + (table?.name ?? ""));
}

function TableSplitView(props: {
  schemas: ListSchemasResponse;
  schemaRefetch: () => Promise<void>;
}) {
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
  const selectedTable = () =>
    pickInitiallySelectedTable(filteredTablesAndViews(), params.table);

  const First = (p: { horizontal: boolean }) => (
    <TablePickerPane
      horizontal={p.horizontal}
      tablesAndViews={filteredTablesAndViews()}
      allTables={props.schemas.tables}
      selectedTable={selectedTable()}
      schemaRefetch={props.schemaRefetch}
    />
  );
  const Second = () => (
    <Show
      when={selectedTable() !== undefined}
      fallback={<div class="m-4">No table selected</div>}
    >
      <TablePane
        selectedTable={selectedTable()!}
        schemas={props.schemas}
        schemaRefetch={props.schemaRefetch}
      />
    </Show>
  );

  return <SplitView first={First} second={Second} />;
}

export function TablePage() {
  const [schemaFetch, { refetch }] = createResource(getAllTableSchemas);
  const schemaRefetch = async () => {
    const schemas = await refetch();
    console.debug("All table schemas re-fetched:", schemas);
  };

  return (
    <Switch>
      <Match when={schemaFetch.error}>
        <span>Schema fetch error: {JSON.stringify(schemaFetch.latest)}</span>
      </Match>

      <Match when={schemaFetch()}>
        <TableSplitView
          schemas={schemaFetch()!}
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
