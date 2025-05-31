import { Suspense, Switch, Match, Index } from "solid-js";
import { createForm } from "@tanstack/solid-form";
import { useQuery } from "@tanstack/solid-query";
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from "@/components/ui/accordion";
import { Card, CardContent, CardHeader } from "@/components/ui/card";

import { adminFetch } from "@/lib/fetch";

import type { UpdateJsonSchemaRequest } from "@bindings/UpdateJsonSchemaRequest";
import type { ListJsonSchemasResponse } from "@bindings/ListJsonSchemasResponse";
import type { JsonSchema } from "@bindings/JsonSchema";

async function listSchemas(): Promise<ListJsonSchemasResponse> {
  const response = await adminFetch("/schema", {
    method: "GET",
  });
  return await response.json();
}

async function _updateSchema(request: UpdateJsonSchemaRequest): Promise<void> {
  await adminFetch("/schema", {
    method: "POST",
    body: JSON.stringify(request),
  });
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
      throw `NOT IMPLEMENTED: ${value}`;

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
      onSubmit={(e) => {
        e.preventDefault();
        e.stopPropagation();
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
                        {schema.name} {schema.builtin ? "<builtin>" : null}
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
              <h2>Schemas</h2>
            </CardHeader>

            <CardContent>
              <p class="text-sm">
                Registering custom JSON schemas is not yet available in the UI.
                However, you can register your own schemas in the{" "}
                <span class="font-mono">`config.textproto`</span>. JSON schemas
                can be used to enforce constraints on TEXT/JSON columns, e.g.:
              </p>

              <pre class="my-2 text-sm">
                CREATE TABLE table (<br />
                &nbsp;&nbsp; json &nbsp;&nbsp;&nbsp;&nbsp; TEXT
                CHECK(jsonschema('mySchema', json))
                <br />
                ) strict;
                <br />
              </pre>

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
