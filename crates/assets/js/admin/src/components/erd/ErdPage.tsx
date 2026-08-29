import {
  Switch,
  Match,
  For,
  Show,
  createEffect,
  createMemo,
  createSignal,
} from "solid-js";
import { createTableSchemaQuery } from "@/lib/api/table";
import { prettyFormatQualifiedName } from "@/lib/schema";
import { NodeMetadata, EdgeMetadata } from "@antv/x6";
import { PortMetadata } from "@antv/x6/lib/model/port";
import {
  TbOutlinePlus,
  TbOutlineMinus,
  TbOutlineMaximize,
  TbOutlineRefresh,
} from "solid-icons/tb";

import { Button } from "@/components/ui/button";
import { Toggle } from "@/components/ui/toggle";
import { Badge } from "@/components/ui/badge";
import { Header } from "@/components/Header";
import {
  ErdGraph,
  nodeName,
  type ErdGraphHandle,
  NODE_WIDTH,
  LINE_HEIGHT,
} from "@/components/erd/ErdGraph";

import {
  getForeignKey,
  getUnique,
  isNotNull,
  hiddenTable,
  tableType,
  getColumns,
  ForeignKey,
} from "@/lib/schema";
import { createTheme, type ResolvedTheme } from "@/lib/theme";

import type { Table } from "@bindings/Table";
import type { View } from "@bindings/View";
import type { ListSchemasResponse } from "@bindings/ListSchemasResponse";
import { QualifiedName } from "@bindings/QualifiedName";

function namesMatch(a: QualifiedName, b: QualifiedName): boolean {
  if (a.name === b.name) {
    return (a.database_schema ?? "main") === (b.database_schema ?? "main");
  }
  return false;
}

function isUserTable(name: QualifiedName): boolean {
  return namesMatch(name, {
    name: "_user",
    database_schema: "main",
  });
}

function findTargetPortName(
  allTablesAndViews: (Table | View)[],
  foreignKey: ForeignKey,
  databaseSchema: string | null,
): string {
  switch (foreignKey.referred_columns.length) {
    case 0:
      break;
    case 1:
      return `${foreignKey.foreign_table}-${foreignKey.referred_columns[0]}`;
    default:
      return foreignKey.foreign_table;
  }

  for (const tableOrView of allTablesAndViews) {
    if (
      !namesMatch(tableOrView.name, {
        name: foreignKey.foreign_table,
        database_schema: databaseSchema,
      })
    ) {
      continue;
    }

    for (const column of getColumns(tableOrView) ?? []) {
      const unique = getUnique(column.options);
      if (unique?.is_primary ?? false) {
        return `${foreignKey.foreign_table}-${column.name}`;
      }
    }
  }

  return foreignKey.foreign_table;
}

export type ErdEntityType = "table" | "view";

export type ErdEntity = {
  id: string;
  name: string;
  type: ErdEntityType;
};

export type ErdRelation = {
  sourceId: string;
  targetId: string;
};

export type ErdModel = {
  entities: ErdEntity[];
  nodes: NodeMetadata[];
  edges: EdgeMetadata[];
  relations: ErdRelation[];
  tableCount: number;
  viewCount: number;
};

export type ErdVisibility = {
  tables: boolean;
  views: boolean;
};

export function searchErdEntities(
  entities: ErdEntity[],
  query: string,
): ErdEntity[] {
  const normalizedQuery = query.trim().toLocaleLowerCase();
  if (normalizedQuery.length === 0) {
    return entities;
  }

  return entities.filter((entity) =>
    entity.name.toLocaleLowerCase().includes(normalizedQuery),
  );
}

export function relatedEntityIds(
  relations: ErdRelation[],
  selectedId?: string,
): Set<string> {
  const related = new Set<string>();
  if (selectedId === undefined) {
    return related;
  }

  related.add(selectedId);
  for (const relation of relations) {
    if (relation.sourceId === selectedId) {
      related.add(relation.targetId);
    }
    if (relation.targetId === selectedId) {
      related.add(relation.sourceId);
    }
  }
  return related;
}

function edgeCellId(endpoint: EdgeMetadata["source"]): string | undefined {
  if (typeof endpoint === "string") {
    return endpoint;
  }
  if (endpoint !== undefined && endpoint !== null && "cell" in endpoint) {
    return typeof endpoint.cell === "string" ? endpoint.cell : undefined;
  }
  return undefined;
}

export function buildErdModel(
  schema: ListSchemasResponse,
  visibility: ErdVisibility,
  theme?: ResolvedTheme,
): ErdModel {
  const allTablesAndViews = [
    ...schema.tables.map(([table]) => table),
    ...schema.views.map(([view]) => view),
  ];
  const visibleTablesAndViews = allTablesAndViews.filter((tableOrView) => {
    const type = tableType(tableOrView);
    const visibleByType =
      type === "view" ? visibility.views : visibility.tables;
    return (
      visibleByType &&
      (!hiddenTable(tableOrView) || isUserTable(tableOrView.name))
    );
  });
  const entities: ErdEntity[] = visibleTablesAndViews.map((tableOrView) => ({
    id: prettyFormatQualifiedName(tableOrView.name),
    name: prettyFormatQualifiedName(tableOrView.name),
    type: tableType(tableOrView) === "view" ? "view" : "table",
  }));
  const visibleIds = new Set(entities.map((entity) => entity.id));
  const nodes: NodeMetadata[] = [];
  const edges: EdgeMetadata[] = [];
  const resolvedTheme = theme ?? "light";

  for (const tableOrView of visibleTablesAndViews) {
    const [node, nodeEdges] = buildErNode(
      resolvedTheme,
      allTablesAndViews,
      tableOrView,
    );
    nodes.push(node);
    edges.push(
      ...nodeEdges.filter((edge) => {
        const sourceId = edgeCellId(edge.source);
        const targetId = edgeCellId(edge.target);
        return (
          sourceId !== undefined &&
          targetId !== undefined &&
          visibleIds.has(sourceId) &&
          visibleIds.has(targetId)
        );
      }),
    );
  }

  const relations = edges.flatMap((edge) => {
    const sourceId = edgeCellId(edge.source);
    const targetId = edgeCellId(edge.target);
    return sourceId !== undefined && targetId !== undefined
      ? [{ sourceId, targetId }]
      : [];
  });

  return {
    entities,
    nodes,
    edges,
    relations,
    tableCount: entities.filter((entity) => entity.type === "table").length,
    viewCount: entities.filter((entity) => entity.type === "view").length,
  };
}

function buildErNode(
  theme: ResolvedTheme,
  allTablesAndViews: (Table | View)[],
  tableOrView: Table | View,
): [NodeMetadata, EdgeMetadata[]] {
  const BASE_EDGE = {
    shape: "edge",
    // attr: { line: { stroke: edge_color, strokeWidth: 2 } },
    zIndex: 0,
  };

  const name = prettyFormatQualifiedName(tableOrView.name);
  const columns = getColumns(tableOrView) ?? [];

  const view = tableType(tableOrView) === "view";
  const ports: PortMetadata[] = columns.map((column) => {
    const notNull = isNotNull(column.options);
    return {
      // View's can have possibly duplicated column names, so we avoid
      // collisions.
      id: view ? undefined : `${name}-${column.name}`,
      group: "list",
      attrs: {
        portNameLabel: {
          text: `${getUnique(column.options)?.is_primary ? "PK · " : getForeignKey(column.options) ? "FK · " : ""}${column.name}`,
        },
        portTypeLabel: {
          text: notNull ? `${column.data_type}` : `${column.data_type}?`,
          // Offset to make more space for name.
          refX: 180,
        },
      },
    };
  });

  const edges: EdgeMetadata[] = columns
    .map((column) => {
      const foreignKey = getForeignKey(column.options);
      if (foreignKey !== undefined) {
        return {
          source: {
            cell: name,
            port: `${name}-${column.name}`,
          },
          // FIXME: lookup pk if referred columns are not provided. Otherwise can
          // we just point at the node rather than a specific port?
          target: {
            cell: prettyFormatQualifiedName({
              name: foreignKey.foreign_table,
              database_schema: tableOrView.name.database_schema,
            }),
            port: findTargetPortName(
              allTablesAndViews,
              foreignKey,
              tableOrView.name.database_schema,
            ),
          },
          ...BASE_EDGE,
        };
      }
    })
    .filter((e) => e !== undefined);

  const node: NodeMetadata = {
    id: name,
    shape: nodeName(theme),
    label: name,
    attrs: {
      typeLabel: { text: tableType(tableOrView).toUpperCase() },
    },
    width: NODE_WIDTH,
    height: LINE_HEIGHT,
    ports,
    // attr: { line: { stroke: edge_color, strokeWidth: 2 } },
  };

  return [node, edges];
}

export type ErdToolbarProps = {
  entities: ErdEntity[];
  showTables: boolean;
  showViews: boolean;
  selectedId?: string;
  onShowTablesChange: (value: boolean) => void;
  onShowViewsChange: (value: boolean) => void;
  onSelect: (id?: string) => void;
  onZoomIn: () => void;
  onZoomOut: () => void;
  onFit: () => void;
  onReset: () => void;
};

export function ErdToolbar(props: ErdToolbarProps) {
  const [query, setQuery] = createSignal("");
  const [open, setOpen] = createSignal(false);
  const [activeIndex, setActiveIndex] = createSignal(-1);
  const results = createMemo(() => searchErdEntities(props.entities, query()));
  createEffect(() => {
    const length = results().length;
    setActiveIndex(length === 0 ? -1 : Math.min(activeIndex(), length - 1));
  });
  const choose = (entity: ErdEntity) => {
    props.onSelect(entity.id);
    setQuery(entity.name);
    setOpen(false);
  };
  const move = (delta: number) => {
    setActiveIndex(
      Math.max(-1, Math.min(results().length - 1, activeIndex() + delta)),
    );
  };

  return (
    <div class="bg-card flex flex-wrap items-center gap-2 border-b p-2">
      <div class="flex items-center gap-1">
        <Toggle
          pressed={props.showTables}
          title="Toggle tables"
          size="sm"
          aria-label="Tables"
          onChange={(pressed) => props.onShowTablesChange(pressed)}
        >
          Tables{" "}
          <Badge round>
            {props.entities.filter((e) => e.type === "table").length}
          </Badge>
        </Toggle>
        <Toggle
          pressed={props.showViews}
          title="Toggle views"
          size="sm"
          aria-label="Views"
          onChange={(pressed) => props.onShowViewsChange(pressed)}
        >
          Views{" "}
          <Badge round>
            {props.entities.filter((e) => e.type === "view").length}
          </Badge>
        </Toggle>
      </div>
      <div class="relative order-last w-full sm:order-none sm:min-w-64 sm:flex-1">
        <input
          class="bg-background h-9 w-full rounded-md border px-3 text-sm"
          type="search"
          role="combobox"
          aria-label="Search entities"
          aria-autocomplete="list"
          aria-expanded={open()}
          aria-controls="erd-search-results"
          aria-activedescendant={
            activeIndex() >= 0
              ? `erd-search-option-${results()[activeIndex()]?.id}`
              : undefined
          }
          value={query()}
          onInput={(event) => {
            setQuery(event.currentTarget.value);
            setOpen(true);
            setActiveIndex(-1);
          }}
          onFocus={() => setOpen(true)}
          onKeyDown={(event) => {
            if (event.key === "ArrowDown") {
              event.preventDefault();
              move(1);
            } else if (event.key === "ArrowUp") {
              event.preventDefault();
              move(-1);
            } else if (
              event.key === "Enter" &&
              open() &&
              results()[activeIndex()]
            )
              choose(results()[activeIndex()]);
            else if (event.key === "Escape") {
              setOpen(false);
              props.onSelect(undefined);
            }
          }}
        />
        <Show when={open() && results().length > 0}>
          <div
            id="erd-search-results"
            role="listbox"
            class="bg-card absolute z-20 mt-1 max-h-64 w-full overflow-auto rounded-md border p-1 shadow-md"
          >
            <For each={results()}>
              {(entity, index) => (
                <button
                  id={`erd-search-option-${entity.id}`}
                  type="button"
                  role="option"
                  aria-selected={activeIndex() === index()}
                  class="hover:bg-accent flex w-full items-center justify-between rounded px-2 py-1 text-left text-sm"
                  onClick={() => choose(entity)}
                >
                  <span>{entity.name}</span>
                  <Badge variant="outline">{entity.type}</Badge>
                </button>
              )}
            </For>
          </div>
        </Show>
      </div>
      <div class="ml-auto flex gap-1">
        <Button
          size="icon"
          variant="outline"
          aria-label="Zoom in"
          title="Zoom in"
          onClick={props.onZoomIn}
        >
          <TbOutlinePlus />
        </Button>
        <Button
          size="icon"
          variant="outline"
          aria-label="Zoom out"
          title="Zoom out"
          onClick={props.onZoomOut}
        >
          <TbOutlineMinus />
        </Button>
        <Button
          size="sm"
          variant="outline"
          aria-label="Fit view"
          title="Fit view"
          onClick={props.onFit}
        >
          <TbOutlineMaximize /> <span class="hidden md:inline">Fit</span>
        </Button>
        <Button
          size="sm"
          variant="outline"
          aria-label="Reset layout"
          title="Reset layout"
          onClick={props.onReset}
        >
          <TbOutlineRefresh /> <span class="hidden md:inline">Reset</span>
        </Button>
      </div>
    </div>
  );
}

function SchemaErdGraph(props: { schema: ListSchemasResponse }) {
  const theme = createTheme();
  const [visibility, setVisibility] = createSignal<ErdVisibility>({
    tables: true,
    views: true,
  });
  const [selectedId, setSelectedId] = createSignal<string>();
  const model = createMemo(() =>
    buildErdModel(props.schema, visibility(), theme()),
  );
  let graph: ErdGraphHandle | undefined;
  const [status, setStatus] = createSignal("");
  const select = (id?: string) => {
    setSelectedId(id);
    setStatus(id ? `${id} selected. Showing direct relationships.` : "Selection cleared.");
    graph?.focus(id);
  };
  return (
    <div class="flex size-full flex-col">
      <ErdToolbar
        entities={model().entities}
        showTables={visibility().tables}
        showViews={visibility().views}
        selectedId={selectedId()}
        onShowTablesChange={(value) =>
          setVisibility((current) => ({ ...current, tables: value }))
        }
        onShowViewsChange={(value) =>
          setVisibility((current) => ({ ...current, views: value }))
        }
        onSelect={select}
        onZoomIn={() => graph?.zoomIn()}
        onZoomOut={() => graph?.zoomOut()}
        onFit={() => graph?.fit()}
        onReset={() => { graph?.reset(); select(undefined); }}
      />
      <div class="min-h-0 flex-1">
        <ErdGraph
          nodes={model().nodes}
          edges={model().edges}
          relations={model().relations}
          selectedId={selectedId()}
          onSelect={select}
          onMount={(g) => (graph = g)}
        />
        <p class="sr-only" aria-live="polite">{status()}</p>
      </div>
    </div>
  );
}

export function ErdPage() {
  const schemaFetch = createTableSchemaQuery();

  return (
    <div class="flex h-full flex-col">
      <Header title="Schema" />

      <Switch>
        <Match when={schemaFetch.isError}>
          <span>Schema fetch error: {JSON.stringify(schemaFetch.error)}</span>
        </Match>

        <Match when={schemaFetch.data}>
          <SchemaErdGraph schema={schemaFetch.data!} />
        </Match>
      </Switch>
    </div>
  );
}

export default ErdPage;
