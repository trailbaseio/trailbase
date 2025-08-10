import {
  For,
  JSXElement,
  Switch,
  Match,
  createMemo,
  createSignal,
} from "solid-js";
import { createForm } from "@tanstack/solid-form";
import { TbInfoCircle } from "solid-icons/tb";
import { useQueryClient } from "@tanstack/solid-query";

import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from "@/components/ui/accordion";
import { Button, buttonVariants } from "@/components/ui/button";
import { Card, CardContent, CardTitle, CardHeader } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
  DialogFooter,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import {
  HoverCard,
  HoverCardContent,
  HoverCardTrigger,
} from "@/components/ui/hover-card";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { SheetFooter } from "@/components/ui/sheet";
import { SheetContainer } from "@/components/SafeSheet";
import { showToast } from "@/components/ui/toast";
import {
  TextField,
  TextFieldLabel,
  TextFieldInput,
} from "@/components/ui/text-field";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";

import {
  Config,
  ConflictResolutionStrategy,
  PermissionFlag,
  RecordApiConfig,
} from "@proto/config";
import {
  buildTextFormField,
  buildOptionalTextFormField,
} from "@/components/FormFields";
import { createConfigQuery, setConfig } from "@/lib/config";
import { parseSqlExpression } from "@/lib/parse";
import { tableType, getForeignKey, getColumns } from "@/lib/schema";
import { buildDefaultRow } from "@/lib/convert";
import { client } from "@/lib/fetch";

import type { ForeignKey } from "@bindings/ForeignKey";
import type { Table } from "@bindings/Table";
import type { View } from "@bindings/View";

const tablePermissions = {
  Create: PermissionFlag.CREATE,
  "Read/List": PermissionFlag.READ,
  Update: PermissionFlag.UPDATE,
  Delete: PermissionFlag.DELETE,
  Schema: PermissionFlag.SCHEMA,
} as const;

const viewPermissions = {
  "Read/List": PermissionFlag.READ,
  Schema: PermissionFlag.SCHEMA,
} as const;

async function asyncSqlValidator({ value }: { value: string | undefined }) {
  console.debug("Query", value);
  if (value) {
    return parseSqlExpression(value);
  }
}

function AclForm(props: {
  entity: string;
  initial?: PermissionFlag[];
  showHeader: boolean;
  onChange: (v: PermissionFlag[]) => void;
  view: boolean;
}) {
  const [acl, setAcl] = createSignal(new Set(props.initial ?? []));

  return (
    <div class="flex">
      <div
        class="grid w-[300px] items-end gap-2"
        style={{ "grid-template-columns": "auto 1fr 1fr 1fr 1fr 1fr" }}
      >
        {props.showHeader && (
          <For
            each={Object.keys(props.view ? viewPermissions : tablePermissions)}
          >
            {(key, index) => (
              <div
                class="col-span-1 ml-1 text-sm [writing-mode:vertical-rl]"
                style={{ "grid-column-start": index() + 2 }}
              >
                {key}
              </div>
            )}
          </For>
        )}

        <div class="col-span-1 col-start-1 w-[120px]">
          <Label>{props.entity}</Label>
        </div>

        <For
          each={Object.values(props.view ? viewPermissions : tablePermissions)}
        >
          {(perm) => (
            <div class="col-span-1">
              <Checkbox
                checked={acl().has(perm)}
                onChange={(v: boolean) => {
                  const set = acl();
                  if (v) {
                    set.add(perm);
                  } else {
                    set.delete(perm);
                  }

                  setAcl(new Set(set));
                  props.onChange([...set]);
                }}
              />
            </div>
          )}
        </For>
      </div>
    </div>
  );
}

type Field = keyof RecordApiConfig;
interface AccessRule {
  field: Field;
  label: string;
  description: string;
}

const tableAccessRules: AccessRule[] = [
  {
    field: "readAccessRule",
    label: "Read Access:",
    description:
      'Row- and request-level read access (_user_, _row_, _req_): If the table has an "owner"\'s column containing binary user ids, access could be rstricted to the owner by setting \'_row_.owner = _user_\' here. Or if the table as a foreign key to a "group" and a relationship defined in a "membership" table: \'(SELECT 1 FROM membership WHERE group = _row_.group AND user = _user_)\'',
  },
  {
    field: "createAccessRule",
    label: "Create Access:",
    description:
      "Request-level create access validation base on _USER_, _REQ_:",
  },
  {
    field: "updateAccessRule",
    label: "Update Access",
    description:
      "Row- and request level update access based on _USER_, _ROW_, _REQ_:",
  },
  {
    field: "deleteAccessRule",
    label: "Delete Access",
    description:
      "Row- and request level delete access based on _USRE_, _ROW_, _REQ_:",
  },
  {
    field: "schemaAccessRule",
    label: "Schema Access",
    description: "Schema access based on _USER_:",
  },
] as const;

const viewAccessRules: AccessRule[] = [
  {
    field: "readAccessRule",
    label: "Read access:",
    description:
      'Row- and request-level read access (_user_, _row_, _req_): If the table has an "owner"\'s column containing binary user ids, access could be rstricted to the owner by setting \'_row_.owner = _user_\' here. Or if the table as a foreign key to a "group" and a relationship defined in a "membership" table: \'(SELECT 1 FROM membership WHERE group = _row_.group AND user = _user_)\'',
  },
  {
    field: "schemaAccessRule",
    label: "Schema Access",
    description: "Schema access based on _USER_:",
  },
] as const;

function updateRecordApiConfig(
  config: Config,
  recordApiConfig: RecordApiConfig,
): Config {
  const newConfig = Config.fromPartial(config);

  for (const i in newConfig.recordApis) {
    const api = newConfig.recordApis[i];
    if (api.name == recordApiConfig.name) {
      newConfig.recordApis[i] = recordApiConfig;
      return newConfig;
    }
  }

  newConfig.recordApis.push(recordApiConfig);
  return newConfig;
}

function removeRecordApiConfig(
  config: Config,
  tableName: string,
  apiName: string,
): Config {
  const newConfig = Config.fromPartial(config);

  while (true) {
    const index = newConfig.recordApis.findIndex(
      (api) => api.tableName === tableName && api.name === apiName,
    );
    if (index < 0) {
      break;
    }

    newConfig.recordApis.splice(index, 1);
  }

  return newConfig;
}

function ConflictResolutionSrategyToString(
  value: ConflictResolutionStrategy | null,
): string {
  switch (value) {
    case ConflictResolutionStrategy.ABORT:
      return "Abort";
    case ConflictResolutionStrategy.ROLLBACK:
      return "Rollback";
    case ConflictResolutionStrategy.FAIL:
      return "Fail";
    case ConflictResolutionStrategy.IGNORE:
      return "Ignore";
    case ConflictResolutionStrategy.REPLACE:
      return "Replace";
    default:
      return "Undefined";
  }
}

export function getRecordApis(
  config: Config | undefined,
  tableName: string,
): RecordApiConfig[] {
  return (config?.recordApis ?? []).filter(
    (api) => api.tableName === tableName,
  );
}

function newRecordApiDefault(opts: {
  tableName: string;
  apiName: string;
}): RecordApiConfig {
  return {
    name: opts.apiName,
    tableName: opts.tableName,
    aclWorld: [],
    aclAuthenticated: [],
    excludedColumns: [],
    expand: [],
  };
}

export function hasRecordApis(
  config: Config | undefined,
  tableName: string,
): boolean {
  for (const api of config?.recordApis ?? []) {
    if (api.tableName === tableName) {
      return true;
    }
  }
  return false;
}

function StyledHoverCard(props: { children: JSXElement }) {
  return (
    <HoverCard>
      <HoverCardTrigger
        class="size-[32px]"
        as={Button<"button">}
        variant="link"
      >
        <TbInfoCircle />
      </HoverCardTrigger>

      <HoverCardContent class="w-80">{props.children}</HoverCardContent>
    </HoverCard>
  );
}

function getForeignKeyColumns(schema: Table | View): [string, ForeignKey][] {
  function filter([colName, fk]: [string, ForeignKey | undefined]) {
    if (!fk) {
      return false;
    }

    if (colName.startsWith("_")) {
      return false;
    }

    if (fk.foreign_table.startsWith("_")) {
      return false;
    }

    return true;
  }

  return (getColumns(schema) ?? [])
    .map(
      (c) =>
        [c.name, getForeignKey(c.options)] as [string, ForeignKey | undefined],
    )
    .filter(filter) as [string, ForeignKey][];
}

function siteUrl(config: Config | undefined): string {
  return (
    config?.server?.siteUrl ??
    (import.meta.env.DEV ? "http://localhost:4000" : window.location.origin)
  );
}

function CodeBlock(props: { text: string }) {
  return <pre class="text-wrap break-all font-mono text-sm">{props.text}</pre>;
}

function ReadExample(props: { apiName: string; config: Config | undefined }) {
  const text = () => `curl \\
  --header "Content-Type: application/json" \\
  --header "Authorization: Bearer ${client.tokens()?.auth_token}" \\
  --request GET \\
  "${siteUrl(props.config)}/api/records/v1/${props.apiName}/<RECORD_ID>"`;

  return <CodeBlock text={text()} />;
}

function ListExample(props: { apiName: string; config: Config | undefined }) {
  const text = () => `curl \\
  --header "Content-Type: application/json" \\
  --header "Authorization: Bearer ${client.tokens()?.auth_token}" \\
  --request GET \\
  "${siteUrl(props.config)}/api/records/v1/${props.apiName}?limit=5"`;

  return <CodeBlock text={text()} />;
}

function CreateExample(props: {
  apiName: string;
  config: Config | undefined;
  schema: Table;
}) {
  const text = () => `curl \\
  --header "Content-Type: application/json" \\
  --header "Authorization: Bearer ${client.tokens()?.auth_token}" \\
  --request POST \\
  --data '${JSON.stringify(buildDefaultRow(props.schema))}' \\
  "${siteUrl(props.config)}/api/records/v1/${props.apiName}"`;

  return <CodeBlock text={text()} />;
}

function UpdateExample(props: {
  apiName: string;
  config: Config | undefined;
  schema: Table;
}) {
  const text = () => `curl \\
  --header "Content-Type: application/json" \\
  --header "Authorization: Bearer ${client.tokens()?.auth_token}" \\
  --request PATCH \\
  --data '${JSON.stringify(buildDefaultRow(props.schema))}' \\
  "${siteUrl(props.config)}/api/records/v1/${props.apiName}/<RECORD_ID>"`;

  return <CodeBlock text={text()} />;
}

function DeleteExample(props: { apiName: string; config: Config | undefined }) {
  const text = () => `curl \\
  --header "Content-Type: application/json" \\
  --header "Authorization: Bearer ${client.tokens()?.auth_token}" \\
  --request DELETE \\
  "${siteUrl(props.config)}/api/records/v1/${props.apiName}/<RECORD_ID>"`;

  return <CodeBlock text={text()} />;
}

function AddApiDialog(props: {
  tableName: string;
  defaultApiName: string;
  setApi: (api: RecordApiConfig) => void;
}) {
  const [open, setOpen] = createSignal(false);
  const [name, setName] = createSignal<string>(props.defaultApiName);

  const confirm = () => {
    const n = name();
    if (n !== "") {
      props.setApi(
        newRecordApiDefault({ tableName: props.tableName, apiName: name() }),
      );
      setOpen(false);
    }
  };

  return (
    <Dialog
      id="edit-help"
      open={open()}
      onOpenChange={(isOpen: boolean) => setOpen(isOpen)}
    >
      <DialogTrigger class={buttonVariants({ variant: "default" })}>
        Add API
      </DialogTrigger>

      <DialogContent>
        <DialogHeader>
          <DialogTitle>Add New API</DialogTitle>
        </DialogHeader>

        <TextField class="flex items-center gap-2">
          <TextFieldLabel class="w-[100px]">API Name</TextFieldLabel>

          <TextFieldInput
            type={"text"}
            value={name()}
            placeholder={"API Name"}
            onKeyUp={(e: KeyboardEvent) => {
              if (e.key === "Enter") {
                confirm();
              }
            }}
            onInput={(e: Event) => {
              setName((e.target as HTMLInputElement).value);
            }}
          />
        </TextField>

        <DialogFooter>
          <div class="flex w-full justify-between gap-2">
            <Button variant="outline" onClick={() => setOpen(false)}>
              Abort
            </Button>

            <Button onClick={confirm}>Add</Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

export function RecordApiSettingsForm(props: {
  close: () => void;
  markDirty: (dirty?: boolean) => void;
  schema: Table | View;
}) {
  const config = createConfigQuery();
  const existingApis = createMemo(() =>
    getRecordApis(config.data!.config, props.schema.name.name),
  );

  type State = {
    api: RecordApiConfig;
    mode: Mode;
  };

  const defaultApi = () => {
    const first = existingApis()[0];
    if (first) {
      return {
        api: first,
        mode: Mode.Update,
      };
    }
    return undefined;
  };
  const [api, setApi] = createSignal<State | undefined>(defaultApi());

  return (
    <div>
      <div class="flex gap-2">
        <AddApiDialog
          tableName={props.schema.name.name ?? "<missing>"}
          defaultApiName={(() => {
            const unqualifiedTableName = props.schema.name.name;
            for (const api of existingApis()) {
              if (api.name === unqualifiedTableName) {
                return "";
              }
            }
            return unqualifiedTableName;
          })()}
          setApi={(api) => {
            setApi({
              api,
              mode: Mode.Create,
            });
          }}
        />

        <Select
          multiple={false}
          placeholder="Select API..."
          value={(() => {
            const name = api()?.api.name;
            for (const api of existingApis()) {
              if (api.name === name) {
                return name;
              }
            }
            return undefined;
          })()}
          options={existingApis().map((api) => api.name)}
          onChange={(apiName: string | null) => {
            if (apiName === null || api()?.api.name === apiName) {
              return;
            }

            for (const api of existingApis()) {
              if (api.name === apiName) {
                setApi({
                  api,
                  mode: Mode.Update,
                });
                break;
              }
            }
          }}
          itemComponent={(props) => (
            <SelectItem item={props.item}>{props.item.rawValue}</SelectItem>
          )}
        >
          <SelectTrigger class="w-[180px]">
            <SelectValue<string>>
              {(state) => state.selectedOption()}
            </SelectValue>
          </SelectTrigger>

          <SelectContent />
        </Select>
      </div>

      <Switch>
        <Match when={api() !== undefined}>
          <IndividualRecordApiSettingsForm
            api={api()!.api}
            mode={api()!.mode}
            reset={(api?: RecordApiConfig) => {
              if (api) {
                setApi({
                  api,
                  mode: Mode.Update,
                });
              } else {
                setApi(defaultApi());
              }
            }}
            {...props}
          />
        </Match>
      </Switch>
    </div>
  );
}

enum Mode {
  Create,
  Update,
}

function IndividualRecordApiSettingsForm(props: {
  reset: (api?: RecordApiConfig) => void;
  markDirty: (dirty?: boolean) => void;
  schema: Table | View;
  api: RecordApiConfig;
  mode: Mode;
}) {
  const queryClient = useQueryClient();
  const config = createConfigQuery();

  const type = () => tableType(props.schema);
  const foreignKeys = () => getForeignKeyColumns(props.schema);

  const form = createForm(() => {
    return {
      defaultValues: props.api,
      onSubmit: async ({ value }: { value: RecordApiConfig }) => {
        console.debug("Add record api config:", value);

        const c = config.data?.config;
        if (!c) {
          console.error("missing base configuration");
          return;
        }

        const newConfig = updateRecordApiConfig(c, value);
        try {
          await setConfig(queryClient, newConfig);

          showToast({
            title: "Success",
            description:
              props.mode === Mode.Create ? "API Added" : "API Updated",
            variant: "success",
          });
          props.reset(value);
          props.markDirty(false);
        } catch (err) {
          showToast({
            title: "Uncaught Error",
            description: `${err}`,
            variant: "error",
          });
        }
      },
    };
  });

  form.useStore((state) => {
    if (state.isDirty && !state.isSubmitted) {
      props.markDirty(true);
    }
  });

  const SubmitDisableButtons = () => {
    return (
      <SheetFooter class="pb-1">
        <Button
          variant="destructive"
          onClick={() => {
            const apiName = props.api.name;
            if (props.mode === Mode.Create) {
              // Abort case
              showToast({
                title: "Success",
                description: `API "${apiName}" discarded`,
                variant: "success",
              });
              props.reset();
              return;
            }

            // Delete case
            const tableName = props.schema.name;
            console.debug("Remove record API config for:", tableName);

            const c = config.data?.config;
            if (!c) {
              console.error("missing base configuration");
              return;
            }

            if (apiName !== undefined) {
              const newConfig = removeRecordApiConfig(
                c,
                tableName.name,
                apiName,
              );

              setConfig(queryClient, newConfig)
                // eslint-disable-next-line solid/reactivity
                .then(() => {
                  showToast({
                    title: "Success",
                    description: `API "${apiName}" deleted`,
                    variant: "success",
                  });
                  props.reset();
                })
                .catch(console.error);
            }
          }}
        >
          {props.mode === Mode.Create ? "Discard" : "Delete"}
        </Button>

        <form.Subscribe
          selector={(state) => ({
            canSubmit: state.canSubmit,
            isSubmitting: state.isSubmitting,
          })}
        >
          {(state) => (
            <Button
              type="submit"
              disabled={!state().canSubmit}
              variant="default"
            >
              {props.mode === Mode.Update ? "Update" : "Create"}
            </Button>
          )}
        </form.Subscribe>
      </SheetFooter>
    );
  };

  return (
    <SheetContainer>
      <form
        method="dialog"
        class="flex flex-col gap-2"
        onSubmit={(e: SubmitEvent) => {
          e.preventDefault();
          form.handleSubmit();
        }}
      >
        <Tabs defaultValue="account" class="w-full">
          <TabsList class="grid w-full grid-cols-3">
            <TabsTrigger value="settings">Settings</TabsTrigger>
            <TabsTrigger value="access">Access</TabsTrigger>
            <TabsTrigger value="examples">Examples</TabsTrigger>
          </TabsList>

          <TabsContent value="settings" class="flex flex-col gap-2">
            <Card>
              <CardContent class="mt-8 flex flex-col gap-4">
                <form.Field
                  name="name"
                  validators={{
                    onChange: ({ value }: { value: string | undefined }) => {
                      return value ? undefined : "Api name missing";
                    },
                  }}
                >
                  {buildTextFormField({
                    disabled: true,
                    label: () => (
                      <div class={labelWidth}>
                        <Label>API Name</Label>
                        <StyledHoverCard>
                          <div class="flex justify-between space-x-4">
                            <div class="space-y-1 text-sm">
                              Public name used to access the API via{" "}
                              <span class="font-mono">
                                /api/records/v1/name
                              </span>
                              .
                            </div>
                          </div>
                        </StyledHoverCard>
                      </div>
                    ),
                  })}
                </form.Field>

                {type() === "table" && (
                  <>
                    <form.Field name="conflictResolution">
                      {(field) => (
                        <div class="flex items-center justify-between gap-2">
                          <div>
                            <Label>Conflict Resolution</Label>
                            <StyledHoverCard>
                              <div class="flex justify-between space-x-4">
                                <div class="space-y-1 text-sm">
                                  SQLite conflict resolution strategy to employ
                                  on record collision.
                                </div>
                              </div>
                            </StyledHoverCard>
                          </div>

                          <Select
                            multiple={false}
                            placeholder="Select group..."
                            defaultValue={field().state.value}
                            options={[
                              ConflictResolutionStrategy.CONFLICT_RESOLUTION_STRATEGY_UNDEFINED,
                              ConflictResolutionStrategy.ABORT,
                              ConflictResolutionStrategy.ROLLBACK,
                              ConflictResolutionStrategy.FAIL,
                              ConflictResolutionStrategy.IGNORE,
                              ConflictResolutionStrategy.REPLACE,
                            ]}
                            onChange={(
                              strategy: ConflictResolutionStrategy | null,
                            ) => {
                              if (
                                strategy === null ||
                                strategy ==
                                  ConflictResolutionStrategy.CONFLICT_RESOLUTION_STRATEGY_UNDEFINED
                              ) {
                                field().handleChange(undefined);
                              } else {
                                field().handleChange(strategy);
                              }
                            }}
                            itemComponent={(props) => (
                              <SelectItem item={props.item}>
                                {ConflictResolutionSrategyToString(
                                  props.item.rawValue,
                                )}
                              </SelectItem>
                            )}
                          >
                            <SelectTrigger class="w-[180px]">
                              <SelectValue<ConflictResolutionStrategy>>
                                {(state) =>
                                  ConflictResolutionSrategyToString(
                                    state.selectedOption(),
                                  )
                                }
                              </SelectValue>
                            </SelectTrigger>

                            <SelectContent />
                          </Select>
                        </div>
                      )}
                    </form.Field>

                    <form.Field
                      name="autofillMissingUserIdColumns"
                      children={(field) => {
                        // TODO: Should be buildBoolFormField?
                        const v = () => field().state.value;
                        return (
                          <div class="mt-2 flex items-center justify-between gap-2">
                            <div>
                              <Label>Infer Missing User</Label>
                              <StyledHoverCard>
                                <div class="flex justify-between space-x-4">
                                  <div class="space-y-1">
                                    <p class="text-sm">
                                      When enabled, user id values not provided
                                      as part of a CREATE request will be
                                      auto-filled using the calling user's
                                      authentication context.
                                    </p>

                                    <p class="text-sm">
                                      For most use-cases this setting should be
                                      off with user ids being provided
                                      explicitly by the client. This can be
                                      useful for static HTML forms.
                                    </p>
                                  </div>
                                </div>
                              </StyledHoverCard>
                            </div>

                            <Checkbox
                              checked={v()}
                              onChange={(v: boolean) => field().handleChange(v)}
                            />
                          </div>
                        );
                      }}
                    />

                    <form.Field
                      name="enableSubscriptions"
                      children={(field) => {
                        const v = () => field().state.value;
                        return (
                          <div class="mt-2 flex items-center justify-between gap-2">
                            <div>
                              <Label>Enable Subscriptions</Label>
                              <StyledHoverCard>
                                <div class="flex justify-between space-x-4">
                                  <div class="space-y-1">
                                    <p class="text-sm">
                                      When enabled, users can subscribe to data
                                      changes in real time. Record access is
                                      checked on a per-record level at
                                      notification time ensuring up-to-date
                                      enforcement as data evolves.
                                    </p>
                                  </div>
                                </div>
                              </StyledHoverCard>
                            </div>

                            <Checkbox
                              checked={v()}
                              onChange={(v: boolean) => field().handleChange(v)}
                            />
                          </div>
                        );
                      }}
                    />

                    <form.Field name="expand">
                      {(field) => {
                        const has = (colName: string) =>
                          new Set([...field().state.value]).has(colName);
                        const add = (colName: string) =>
                          field().handleChange(
                            Array.from(
                              new Set([colName, ...field().state.value]),
                            ),
                          );
                        const remove = (colName: string) => {
                          const s = new Set(field().state.value);
                          s.delete(colName);
                          field().handleChange(Array.from(s));
                        };

                        return (
                          <For each={foreignKeys()}>
                            {([colName, item]) => {
                              return (
                                <div class="mt-2 flex items-center justify-between gap-2">
                                  <div>
                                    <Label>
                                      Expand Column ({colName} {"=>"}{" "}
                                      {item.foreign_table})
                                    </Label>
                                    <StyledHoverCard>
                                      <div class="flex justify-between space-x-4">
                                        <div class="space-y-1 text-sm">
                                          Expanding a foreign key column,
                                          changes the APIs field schema from
                                          simply being the foreign key, to
                                          <span class="font-mono">{`{ id: any, data?: object }`}</span>
                                          . Then the respective foreign record
                                          can be included during read/list by
                                          specifying
                                          <span class="font-mono">
                                            ?expand={colName}
                                          </span>
                                          .
                                        </div>
                                      </div>
                                    </StyledHoverCard>
                                  </div>

                                  <Checkbox
                                    checked={has(colName)}
                                    onChange={(v: boolean) =>
                                      v ? add(colName) : remove(colName)
                                    }
                                  />
                                </div>
                              );
                            }}
                          </For>
                        );
                      }}
                    </form.Field>
                  </>
                )}
              </CardContent>
            </Card>

            <SubmitDisableButtons />
          </TabsContent>

          <TabsContent value="access" class="flex flex-col gap-2">
            <Card>
              <CardHeader>
                <CardTitle>ACL</CardTitle>
              </CardHeader>

              <CardContent class="flex flex-col gap-4">
                <p class="text-sm">
                  Grant access to specific API actions for authorized users or
                  anyone using the following access-control-list (ACL). By
                  default, actions are inaccessible.
                </p>

                <form.Field name="aclWorld">
                  {(field) => {
                    const v = field().state.value;
                    return (
                      <div class="mb-4">
                        <AclForm
                          entity="World"
                          showHeader={true}
                          initial={v}
                          onChange={field().handleChange}
                          view={type() === "view"}
                        />
                      </div>
                    );
                  }}
                </form.Field>

                <form.Field name="aclAuthenticated">
                  {(field) => {
                    const v = field().state.value;
                    return (
                      <div class="mb-4">
                        <AclForm
                          entity="Authenticated"
                          showHeader={false}
                          initial={v}
                          onChange={field().handleChange}
                          view={type() === "view"}
                        />
                      </div>
                    );
                  }}
                </form.Field>
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <CardTitle>Access Rules</CardTitle>
              </CardHeader>

              <CardContent class="flex flex-col gap-4">
                <p class="text-sm">
                  In addition to coarse ACLs, access can be constrained using
                  custom SQL expressions. Check the{" "}
                  <a
                    class="underline"
                    href="https://trailbase.io/documentation/apis_record/#permissions"
                  >
                    docs
                  </a>{" "}
                  for more information. Example:
                </p>

                <pre class="pl-2 font-mono text-sm">{exampleRule}</pre>

                <For
                  each={type() === "view" ? viewAccessRules : tableAccessRules}
                >
                  {(item) => {
                    return (
                      <form.Field
                        name={item.field}
                        validators={{
                          onChangeAsync: asyncSqlValidator,
                          onChangeAsyncDebounceMs: 500,
                        }}
                      >
                        {buildOptionalTextFormField({
                          label: () => (
                            <div class={labelWidth}>{item.label}</div>
                          ),
                        })}
                      </form.Field>
                    );
                  }}
                </For>
              </CardContent>
            </Card>

            <SubmitDisableButtons />
          </TabsContent>

          <TabsContent value="examples">
            <Card>
              <CardContent class="mt-8 flex flex-col gap-4">
                <p class="text-sm">
                  Some examples on how to interact with the APIs using{" "}
                  <span class="font-mono">curl</span>. Make sure to provide
                  access first. Note further that access tokens are short-lived
                  and expire frequently.
                </p>

                <Accordion multiple={false} collapsible class="w-full">
                  <AccordionItem value="read">
                    <AccordionTrigger>Read Record</AccordionTrigger>

                    <AccordionContent>
                      <ReadExample
                        apiName={form.state.values.name ?? ""}
                        config={config.data?.config}
                      />
                    </AccordionContent>
                  </AccordionItem>

                  <AccordionItem value="list">
                    <AccordionTrigger>List Records</AccordionTrigger>

                    <AccordionContent>
                      <ListExample
                        apiName={form.state.values.name ?? ""}
                        config={config.data?.config}
                      />
                    </AccordionContent>
                  </AccordionItem>

                  {type() === "table" && (
                    <>
                      <AccordionItem value="create">
                        <AccordionTrigger>Create Record</AccordionTrigger>

                        <AccordionContent>
                          <CreateExample
                            apiName={form.state.values.name ?? ""}
                            config={config.data?.config}
                            schema={props.schema as Table}
                          />
                        </AccordionContent>
                      </AccordionItem>

                      <AccordionItem value="update">
                        <AccordionTrigger>Update Record</AccordionTrigger>

                        <AccordionContent>
                          <UpdateExample
                            apiName={form.state.values.name ?? ""}
                            config={config.data?.config}
                            schema={props.schema as Table}
                          />
                        </AccordionContent>
                      </AccordionItem>

                      <AccordionItem value="delete">
                        <AccordionTrigger>Delete Record</AccordionTrigger>

                        <AccordionContent>
                          <DeleteExample
                            apiName={form.state.values.name ?? ""}
                            config={config.data?.config}
                          />
                        </AccordionContent>
                      </AccordionItem>
                    </>
                  )}
                </Accordion>
              </CardContent>
            </Card>
          </TabsContent>
        </Tabs>
      </form>
    </SheetContainer>
  );
}

const labelWidth = "w-[112px]";
const exampleRule = `EXISTS(
  SELECT 1
  FROM group AS g
  WHERE
    g.member = _USER_.id AND g.name = 'mygroup'
)`;
