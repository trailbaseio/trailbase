import { Switch, Match, createMemo } from "solid-js";
import { createTableSchemaQuery } from "@/lib/api/table";
import { prettyFormatQualifiedName } from "@/lib/schema";
import { Graph, NodeMetadata, EdgeMetadata } from "@antv/x6";
import { PortMetadata } from "@antv/x6/lib/model/port";
import { TbPlus, TbMinus } from "solid-icons/tb";

import { Button } from "@/components/ui/button";
import { Header } from "@/components/Header";
import {
  ErdGraph,
  ER_NODE_NAME,
  NODE_WIDTH,
  LINE_HEIGHT,
  EDGE_COLOR,
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
import { createIsMobile } from "@/lib/signals";

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

function buildErNode(
  allTablesAndViews: (Table | View)[],
  tableOrView: Table | View,
): [NodeMetadata, EdgeMetadata[]] {
  const BASE_EDGE = {
    shape: "edge",
    attr: { line: { stroke: EDGE_COLOR, strokeWidth: 2 } },
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
    shape: ER_NODE_NAME,
    label: `${name} [${tableType(tableOrView)}]`,
    width: NODE_WIDTH,
    height: LINE_HEIGHT,
    ports,
    attr: { line: { stroke: EDGE_COLOR, strokeWidth: 2 } },
  };

  return [node, edges];
}

function SchemaErdGraph(props: { schema: ListSchemasResponse }) {
  const isMobile = createIsMobile();

  const nodesAndEdges = createMemo(() => {
    const nodes: NodeMetadata[] = [];
    const edges: EdgeMetadata[] = [];

    const allTablesAndViews = [...props.schema.tables, ...props.schema.views];
    for (const tableOrView of allTablesAndViews) {
      if (hiddenTable(tableOrView) && !isUserTable(tableOrView.name)) {
        continue;
      }

      const [n, e] = buildErNode(allTablesAndViews, tableOrView);
      nodes.push(n);
      edges.push(...e);
    }

    return { nodes, edges };
  });

  const style = () => {
    if (isMobile()) {
      // Header (65px) + Navbar (48px) = 113px
      return "h-[calc(100dvh-113px)] w-[calc(100dvw)]";
    }
    return "h-[calc(100dvh-65px)] w-[calc(100dvw-58px)]";
  };

  let graph: Graph | undefined;

  return (
    <div class={style()}>
      {/* UI overlay */}
      <div class="absolute right-0 z-10">
        <div class="m-2 flex flex-col gap-2">
          <Button
            size="icon"
            variant="outline"
            class="bg-card"
            onClick={() => {
              if (graph !== undefined) {
                graph.zoomTo(graph.zoom() * 2);
              }
            }}
          >
            <TbPlus />
          </Button>

          <Button
            size="icon"
            variant="outline"
            class="bg-card"
            onClick={() => {
              if (graph !== undefined) {
                graph.zoomTo(graph.zoom() / 2);
              }
            }}
          >
            <TbMinus />
          </Button>
        </div>
      </div>

      <ErdGraph
        nodes={nodesAndEdges().nodes}
        edges={nodesAndEdges().edges}
        onMount={(g) => (graph = g)}
      />
    </div>
  );
}

export function ErdPage() {
  const schemaFetch = createTableSchemaQuery();

  return (
    <div class="flex h-full flex-col">
      <Header title="Schema" class="h-[65px]" />

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
