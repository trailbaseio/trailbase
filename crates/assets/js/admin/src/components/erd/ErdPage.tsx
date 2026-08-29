import { Switch, Match, createMemo } from "solid-js";
import { createTableSchemaQuery } from "@/lib/api/table";
import { prettyFormatQualifiedName } from "@/lib/schema";
import { Graph, NodeMetadata, EdgeMetadata } from "@antv/x6";
import { PortMetadata } from "@antv/x6/lib/model/port";
import { TbOutlinePlus, TbOutlineMinus } from "solid-icons/tb";

import { Button } from "@/components/ui/button";
import { Header } from "@/components/Header";
import {
  ErdGraph,
  nodeName,
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
    const visibleByType = type === "view" ? visibility.views : visibility.tables;
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
          text: column.name,
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
    label: `${name} [${tableType(tableOrView)}]`,
    width: NODE_WIDTH,
    height: LINE_HEIGHT,
    ports,
    // attr: { line: { stroke: edge_color, strokeWidth: 2 } },
  };

  return [node, edges];
}

function SchemaErdGraph(props: { schema: ListSchemasResponse }) {
  const theme = createTheme();

  const model = createMemo(() =>
    buildErdModel(props.schema, { tables: true, views: true }, theme()),
  );

  let graph: Graph | undefined;

  return (
    <div class="size-full">
      {/* UI overlay */}
      <div class="absolute right-0 z-10">
        <div class="m-2 flex flex-col gap-2">
          <Button
            size="icon"
            variant="outline"
            class="bg-card"
            aria-label="Zoom in"
            onClick={() => {
              if (graph !== undefined) {
                graph.zoomTo(graph.zoom() * 2);
              }
            }}
          >
            <TbOutlinePlus />
          </Button>

          <Button
            size="icon"
            variant="outline"
            class="bg-card"
            aria-label="Zoom out"
            onClick={() => {
              if (graph !== undefined) {
                graph.zoomTo(graph.zoom() / 2);
              }
            }}
          >
            <TbOutlineMinus />
          </Button>
        </div>
      </div>

      <ErdGraph
        nodes={model().nodes}
        edges={model().edges}
        onMount={(g) => (graph = g)}
      />
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
