import {
  type Signal,
  For,
  Match,
  Show,
  Switch,
  createMemo,
  createEffect,
  createResource,
  createSignal,
} from "solid-js";
import { useSearchParams } from "@solidjs/router";
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
  selectedTable: Signal<Table | View | undefined>;
  schemaRefetch: () => Promise<void>;
}) {
  const showHidden = useStore($showHiddenTables);
  const tablesAndViews = createMemo(() =>
    props.tablesAndViews.toSorted(tableCompare),
  );

  const [selectedTable, setSelectedTable] = props.selectedTable;

  const horizontal = () => props.horizontal;

  return (
    <div
      class={`${horizontal() ? "flex flex-col h-dvh" : "flex"} gap-2 overflow-scroll hide-scrollbars p-4`}
    >
      <SwitchUi
        class="flex items-center justify-center gap-2"
        checked={showHidden()}
        onChange={(show: boolean) => {
          const current = selectedTable();
          if (!show && current && hiddenTable(current)) {
            setSelectedTable(undefined);
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
                    const table = tablesAndViews().find(
                      (t) => t.name === tableName,
                    );
                    if (table) {
                      setSelectedTable(table);
                    }
                  }}
                  {...sheet}
                />
              </SheetContent>

              <SheetTrigger
                as={(props: DialogTriggerProps) => (
                  <Button
                    class="gap-2 min-w-[100px]"
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

      <For each={tablesAndViews()}>
        {(item: Table | View) => {
          const hidden = hiddenTable(item);
          const type = tableType(item);
          const selected = () => item.name === selectedTable()?.name;

          return (
            <Button
              variant={selected() ? "default" : "outline"}
              onClick={() => setSelectedTable(item)}
              class="flex gap-2"
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

function TableSplitView(props: {
  schemas: ListSchemasResponse;
  schemaRefetch: () => Promise<void>;
}) {
  const showHidden = useStore($showHiddenTables);
  const filteredTablesAndViews = createMemo(() => {
    const all = [...props.schemas.tables, ...props.schemas.views];
    const show = showHidden();
    if (show) {
      return all;
    }
    return all.filter((t) => !hiddenTable(t));
  });

  const [searchParams, setSearchParams] = useSearchParams<{ table: string }>();
  const [selectedTable, setSelectedTable] = createSignal<
    Table | View | undefined
  >(pickInitiallySelectedTable(filteredTablesAndViews(), searchParams.table));
  createEffect(() => {
    // Update search params.
    setSearchParams({ table: selectedTable()?.name });
  });

  const First = (p: { horizontal: boolean }) => (
    <TablePickerPane
      horizontal={p.horizontal}
      tablesAndViews={filteredTablesAndViews()}
      allTables={props.schemas.tables}
      selectedTable={[selectedTable, setSelectedTable]}
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

export function TablesPage() {
  const [schemaFetch, { refetch }] = createResource(getAllTableSchemas);

  return (
    <Switch>
      <Match when={schemaFetch.error}>
        <span>Schema fetch error: {JSON.stringify(schemaFetch.latest)}</span>
      </Match>

      <Match when={schemaFetch()}>
        <TableSplitView
          schemas={schemaFetch()!}
          schemaRefetch={async () => {
            const schemas = await refetch();
            console.debug("All table schemas re-fetched:", schemas);
          }}
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
