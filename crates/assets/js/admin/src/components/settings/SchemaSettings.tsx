import { Suspense, Show, Switch, Match, Index } from "solid-js";
import { createForm } from "@tanstack/solid-form";
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

import type { ListJsonSchemasResponse } from "@bindings/ListJsonSchemasResponse";
import type { JsonSchema } from "@bindings/JsonSchema";

async function listSchemas(): Promise<ListJsonSchemasResponse> {
  const response = await adminFetch("/schema", {
    method: "GET",
  });
  return await response.json();
}

function SchemaSettingsForm(props: {
  markDirty: () => void;
  postSubmit: () => void;
  schemas: JsonSchema[];
}) {
  const form = createForm(() => ({
    defaultValues: {
      entries: props.schemas,
    },
    onSubmit: async ({ value }) => {
      throw new Error(`NOT IMPLEMENTED: ${value}`);

      props.postSubmit();
    },
  }));

  form.useStore((state) => {
    if (state.isDirty && !state.isSubmitted) {
      props.markDirty();
    }
  });

  return (
    <form
      method="dialog"
      onSubmit={(e: SubmitEvent) => {
        e.preventDefault();
        form.handleSubmit();
      }}
    >
      <form.Field name="entries" mode="array">
        {(field) => {
          return (
            <Accordion multiple={false} collapsible class="w-full">
              <Index each={field().state.value}>
                {(_, i) => {
                  const schema = field().state.value[i];
                  return (
                    <AccordionItem value={`item-${i}`}>
                      <AccordionTrigger>
                        <span class="flex gap-2">
                          {schema.name}

                          <Show when={schema.builtin}>
                            <Badge variant="outline">built-in</Badge>
                          </Show>
                        </span>
                      </AccordionTrigger>

                      <AccordionContent>
                        <form.Field name={`entries[${i}].schema`}>
                          {(subField) => (
                            <pre>
                              {JSON.stringify(
                                JSON.parse(subField().state.value),
                                null,
                                2,
                              )}
                            </pre>
                          )}
                        </form.Field>
                      </AccordionContent>
                    </AccordionItem>
                  );
                }}
              </Index>
            </Accordion>
          );
        }}
      </form.Field>
    </form>
  );
}

export function SchemaSettings(props: {
  markDirty: () => void;
  postSubmit: () => void;
}) {
  const schemas = useQuery(() => ({
    queryKey: ["admin", "jsonSchemas"],
    queryFn: listSchemas,
  }));
  return (
    <Suspense fallback={<div>Loading...</div>}>
      <Switch>
        <Match when={schemas.isError}>
          <span>Error: {`${schemas.error}`}</span>
        </Match>

        <Match when={schemas.isSuccess}>
          <Card>
            <CardHeader>
              <h2>JSON Schemas</h2>
            </CardHeader>

            <CardContent>
              <p class="text-sm">
                Custom JSON schemas can be registered to enforce constraints on
                columns of your database tables, e.g.:
              </p>

              <pre class="my-4 overflow-x-auto text-sm">{exampleTable}</pre>

              <p class="text-sm">
                Note, registration via the admin UI is not yet available. You
                can register custom schemas in your instance's{" "}
                <span class="font-mono text-nowrap">
                  `{"<"}traildepot{">"}/config.textproto`
                </span>{" "}
                and they will show up here.
              </p>
              <SchemaSettingsForm
                markDirty={props.markDirty}
                postSubmit={props.postSubmit}
                schemas={schemas.data?.schemas ?? []}
              />
            </CardContent>
          </Card>
        </Match>
      </Switch>
    </Suspense>
  );
}

const exampleTable =
  "CREATE TABLE 'table' (\n  json    TEXT CHECK(jsonschema('mySchema', json)) \n) STRICT;";
