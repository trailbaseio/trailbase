import {
  For,
  Match,
  Show,
  Switch,
  createEffect,
  createMemo,
  createSignal,
} from "solid-js";
import { useQuery } from "@tanstack/solid-query";
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from "@/components/ui/accordion";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { adminFetch } from "@/lib/fetch";
import { createSystemInfoQuery } from "@/lib/api/info";
import type { ListJsonSchemasResponse } from "@bindings/ListJsonSchemasResponse";

async function listSchemas(): Promise<ListJsonSchemasResponse> {
  const response = await adminFetch("/schema", { method: "GET" });
  return response.json();
}

export function formatSchemaSource(source: string): string {
  try {
    return JSON.stringify(JSON.parse(source), null, 2);
  } catch {
    return source;
  }
}

export function SchemaSettings(props: {
  setDirty: (dirty: boolean) => void;
  postSubmit: () => void;
}) {
  createEffect(() => props.setDirty(false));
  const schemas = useQuery(() => ({
    queryKey: ["admin", "jsonSchemas"],
    queryFn: listSchemas,
  }));
  const systemInfo = createSystemInfoQuery();
  const isPostgres = () => systemInfo.data?.postgres === true;
  const [search, setSearch] = createSignal("");
  const filtered = createMemo(() => {
    const needle = search().trim().toLocaleLowerCase();
    return [...(schemas.data?.schemas ?? [])]
      .sort((a, b) => a.name.localeCompare(b.name))
      .filter((schema) => !isPostgres() || schema.builtin)
      .filter((schema) => schema.name.toLocaleLowerCase().includes(needle));
  });

  return (
    <Switch>
      <Match when={schemas.isLoading || systemInfo.isLoading}>
        <p role="status">Loading schemas...</p>
      </Match>
      <Match when={schemas.isError}>
        <p role="alert">Unable to load schemas. Try again.</p>
      </Match>
      <Match when={systemInfo.isError}>
        <p role="alert">Unable to load system information. Try again.</p>
      </Match>
      <Match when={schemas.isSuccess}>
        <Card>
          <CardHeader>
            <h2>JSON Schemas</h2>
          </CardHeader>
          <CardContent class="flex flex-col gap-4">
            <Show
              when={isPostgres()}
              fallback={
                <>
                  <p class="text-sm">
                    Custom JSON schemas can be registered to enforce constraints
                    on columns of your database tables:
                  </p>
                  <pre class="overflow-x-auto text-sm whitespace-pre-wrap">
                    {exampleTable}
                  </pre>
                  <p class="text-sm">
                    Registration via the admin UI is not yet available. Register
                    custom schemas in your instance&apos;s config.textproto.
                  </p>
                </>
              }
            >
              <p class="text-sm">
                Custom schemas are not supported in Postgres mode. Only the
                following built-ins are available:
              </p>
            </Show>
            <label for="schema-search">Search schemas</label>
            <input
              id="schema-search"
              type="search"
              value={search()}
              placeholder="Search schemas"
              onInput={(event) => setSearch(event.currentTarget.value)}
              class="h-9 w-full rounded-md border bg-transparent px-3 py-2 text-sm"
            />
            <Show
              when={filtered().length > 0}
              fallback={
                <p>
                  {search().trim()
                    ? "No schemas match your search."
                    : "No schemas available."}
                </p>
              }
            >
              <Accordion multiple={false} collapsible class="w-full">
                <For each={filtered()}>
                  {(schema, index) => (
                    <AccordionItem value={`${schema.name}-${index()}`}>
                      <AccordionTrigger>
                        <span class="flex gap-2">
                          {schema.name}
                          <Show when={schema.builtin}>
                            <Badge variant="outline">built-in</Badge>
                          </Show>
                        </span>
                      </AccordionTrigger>
                      <AccordionContent>
                        <pre class="max-w-full overflow-x-auto whitespace-pre-wrap">
                          {formatSchemaSource(schema.schema)}
                        </pre>
                      </AccordionContent>
                    </AccordionItem>
                  )}
                </For>
              </Accordion>
            </Show>
          </CardContent>
        </Card>
      </Match>
    </Switch>
  );
}

const exampleTable =
  "CREATE TABLE 'table' (\n  json TEXT CHECK(jsonschema('mySchema', json))\n) STRICT;";
