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
import type { JsonSchema } from "@bindings/JsonSchema";

async function listSchemas(): Promise<ListJsonSchemasResponse> {
  const response = await adminFetch("/schema", { method: "GET" });
  return response.json();
}

const MAX_SCHEMA_SOURCE_LENGTH = 50_000;
const MAX_SCHEMA_DEPTH = 100;
const MAX_FORMATTED_SCHEMA_LENGTH = MAX_SCHEMA_SOURCE_LENGTH;
const MAX_SCHEMA_NAME_LENGTH = 256;

function hasControlCharacters(value: string): boolean {
  return [...value].some((character) => {
    const code = character.charCodeAt(0);
    return code <= 31 || (code >= 127 && code <= 159);
  });
}

function boundedSource(source: string): string {
  return source.length > MAX_SCHEMA_SOURCE_LENGTH
    ? `${source.slice(0, MAX_SCHEMA_SOURCE_LENGTH)}… truncated`
    : source;
}

function exceedsJsonDepth(source: string): boolean {
  let depth = 0;
  let quoted = false;
  let escaped = false;
  for (const character of source) {
    if (quoted) {
      if (escaped) escaped = false;
      else if (character === "\\") escaped = true;
      else if (character === '"') quoted = false;
      continue;
    }
    if (character === '"') quoted = true;
    else if (character === "{" || character === "[") {
      depth += 1;
      if (depth > MAX_SCHEMA_DEPTH) return true;
    } else if (character === "}" || character === "]") {
      depth = Math.max(0, depth - 1);
    }
  }
  return false;
}

export function formatSchemaSource(source: unknown): string {
  const text = typeof source === "string" ? source : String(source ?? "");
  if (text.length > MAX_SCHEMA_SOURCE_LENGTH || exceedsJsonDepth(text)) {
    return boundedSource(text);
  }
  try {
    const formatted = JSON.stringify(JSON.parse(text), null, 2);
    return formatted.length > MAX_FORMATTED_SCHEMA_LENGTH
      ? boundedSource(text)
      : formatted;
  } catch {
    return boundedSource(text);
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
    const payload: unknown = schemas.data?.schemas;
    const entries = Array.isArray(payload) ? payload : [];
    return entries
      .filter(
        (schema): schema is JsonSchema & { name: string; schema: string } =>
          typeof schema?.name === "string" &&
          schema.name.length > 0 &&
          schema.name.length <= MAX_SCHEMA_NAME_LENGTH &&
          !hasControlCharacters(schema.name) &&
          typeof schema?.schema === "string",
      )
      .sort((a, b) => a.name.localeCompare(b.name))
      .filter((schema) => !isPostgres() || schema.builtin === true)
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
            <p aria-live="polite" role="status">
              {filtered().length > 0
                ? `${filtered().length} schema${filtered().length === 1 ? "" : "s"} shown`
                : search().trim()
                  ? "No schemas match your search."
                  : "No schemas available."}
            </p>
            <Show when={filtered().length > 0}>
              <Accordion multiple={false} collapsible class="w-full">
                <For each={filtered()}>
                  {(schema) => (
                    <AccordionItem value={schema.name}>
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
