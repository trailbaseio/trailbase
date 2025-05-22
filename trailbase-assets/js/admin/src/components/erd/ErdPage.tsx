import { Switch, Match, createMemo } from "solid-js";
import { createTableSchemaQuery } from "@/lib/table";

import { Header } from "@/components/Header";
import {
  ErdGraph,
  NodeMetadata,
  EdgeMetadata,
  PortMetadata,
  ER_NODE_NAME,
  NODE_WIDTH,
  LINE_HEIGHT,
  EDGE_COLOR,
} from "@/components/ErdGraph";

import {
  getForeignKey,
  getUnique,
  isNotNull,
  hiddenTable,
  tableType,
  ForeignKey,
} from "@/lib/schema";

import type { Table } from "@bindings/Table";
import type { View } from "@bindings/View";
import type { ListSchemasResponse } from "@bindings/ListSchemasResponse";

function findTargetPortName(
  allTablesAndViews: (Table | View)[],
  foreignKey: ForeignKey,
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
    if (tableOrView.name.name !== foreignKey.foreign_table) {
      continue;
    }

    for (const column of tableOrView.columns ?? []) {
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

  const columns = tableOrView.columns ?? [];

  const view = tableType(tableOrView) === "view";
  const ports: PortMetadata[] = columns.map((column) => {
    const notNull = isNotNull(column.options);
    return {
      // View's can have possibly duplicated column names, so we avoid
      // collisions.
      id: view ? undefined : `${tableOrView.name}-${column.name}`,
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
            cell: tableOrView.name.name,
            port: `${tableOrView.name}-${column.name}`,
          },
          // FIXME: lookup pk if referred columns are not provided. Otherwise can
          // we just point at the node rather than a specific port?
          target: {
            cell: foreignKey.foreign_table,
            port: findTargetPortName(allTablesAndViews, foreignKey),
          },
          ...BASE_EDGE,
        };
      }
    })
    .filter((e) => e !== undefined);

  const node: NodeMetadata = {
    id: tableOrView.name.name,
    shape: ER_NODE_NAME,
    label: `${tableOrView.name} [${tableType(tableOrView)}]`,
    width: NODE_WIDTH,
    height: LINE_HEIGHT,
    ports,
    attr: { line: { stroke: EDGE_COLOR, strokeWidth: 2 } },
  };

  return [node, edges];
}

function SchemaErdGraph(props: { schema: ListSchemasResponse }) {
  const nodesAndEdges = createMemo(() => {
    const nodes: NodeMetadata[] = [];
    const edges: EdgeMetadata[] = [];

    const allTablesAndViews = [...props.schema.tables, ...props.schema.views];
    for (const tableOrView of allTablesAndViews) {
      if (tableOrView.name.name !== "_user" && hiddenTable(tableOrView)) {
        continue;
      }

      const [n, e] = buildErNode(allTablesAndViews, tableOrView);
      nodes.push(n);
      edges.push(...e);
    }

    return { nodes, edges };
  });

  return (
    <ErdGraph nodes={nodesAndEdges().nodes} edges={nodesAndEdges().edges} />
  );
}

export function ErdPage() {
  const schemaFetch = createTableSchemaQuery();

  return (
    <div class="h-dvh">
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
