import { Switch, Match } from "solid-js";
import { createTableSchemaQuery } from "@/lib/table";

import { Mermaid } from "@/components/Mermaid";
import { Header } from "@/components/Header";

import { getForeignKey, getUnique, isNotNull, hiddenTable } from "@/lib/schema";

import type { Table } from "@bindings/Table";
import type { View } from "@bindings/View";
import type { ListSchemasResponse } from "@bindings/ListSchemasResponse";

function buildErdForTableOrView(
  tableOrView: Table | View,
  entities: string[],
  relations: string[],
) {
  const rel = "references";

  entities.push(`${tableOrView.name} {`);
  for (const column of tableOrView.columns ?? []) {
    const unique = getUnique(column.options);
    const foreignKey = getForeignKey(column.options);
    if (foreignKey !== undefined) {
      const notNull = isNotNull(column.options);
      if (notNull) {
        relations.push(
          `   ${tableOrView.name} 1 to 1 ${foreignKey.foreign_table} : ${rel}`,
        );
      } else {
        relations.push(
          `   ${tableOrView.name} one or zero to 1 ${foreignKey.foreign_table} : ${rel}`,
        );
      }
    }

    const isPrimary = unique?.is_primary ?? false;
    if (isPrimary) {
      if (foreignKey !== undefined) {
        entities.push(
          `   ${column.data_type} ${column.name} PK, FK "${foreignKey.foreign_table}"`,
        );
      } else {
        entities.push(`   ${column.data_type} ${column.name} PK`);
      }
    } else if (foreignKey !== undefined) {
      entities.push(
        `   ${column.data_type} ${column.name} FK "${foreignKey.foreign_table}"`,
      );
    } else {
      entities.push(`   ${column.data_type} ${column.name}`);
    }
  }
  entities.push(`}`);
}

function buildErd(schema: ListSchemasResponse): string {
  const entities: string[] = [];
  const relations: string[] = [];
  for (const table of schema.tables) {
    if (hiddenTable(table)) {
      continue;
    }
    buildErdForTableOrView(table, entities, relations);
  }

  for (const view of schema.views) {
    if (hiddenTable(view)) {
      continue;
    }
    buildErdForTableOrView(view, entities, relations);
  }

  return `
    erDiagram

    ${entities.join("\n")}

    ${relations.join("\n")}
  `;
}

export function ErdPage() {
  const schemaFetch = createTableSchemaQuery();

  return (
    <div class="h-dvh overflow-y-auto">
      <Header title="Schema" />

      <div class="flex flex-col gap-4 p-4">
        <Switch>
          <Match when={schemaFetch.isError}>
            <span>Schema fetch error: {JSON.stringify(schemaFetch.error)}</span>
          </Match>

          <Match when={schemaFetch.data}>
            <Mermaid value={buildErd(schemaFetch.data!)} />
          </Match>
        </Switch>
      </div>
    </div>
  );
}

export default ErdPage;
