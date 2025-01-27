import {
  createResource,
  createSignal,
  For,
  Show,
  Switch,
  Match,
} from "solid-js";
import type { Component, JSXElement } from "solid-js";
import { Route, useNavigate, type RouteSectionProps } from "@solidjs/router";
import { createForm } from "@tanstack/solid-form";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { Dialog } from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import { Separator } from "@/components/ui/separator";
import { TextField, TextFieldLabel } from "@/components/ui/text-field";
import { showToast } from "@/components/ui/toast";

import { Config, ServerConfig } from "@proto/config";
import {
  notEmptyValidator,
  buildNumberFormField,
  buildTextFormField,
  gapStyle,
} from "@/components/FormFields";
import { ConfirmCloseDialog } from "@/components/SafeSheet";
import { AuthSettings } from "@/components/settings/AuthSettings";
import { SchemaSettings } from "@/components/settings/SchemaSettings";
import { EmailSettings } from "@/components/settings/EmailSettings";
import { SplitView } from "@/components/SplitView";

import type { InfoResponse } from "@bindings/InfoResponse";
import { createConfigQuery, setConfig } from "@/lib/config";
import { adminFetch } from "@/lib/fetch";

function ServerSettings(props: CommonProps) {
  const config = createConfigQuery();

  const Form = (p: { config: ServerConfig }) => {
    const form = createForm<ServerConfig>(() => ({
      defaultValues: p.config,
      onSubmit: async ({ value }: { value: ServerConfig }) => {
        const c = config.data?.config;
        if (!c) {
          console.warn("Missing base config:");
          return;
        }

        const newConfig = Config.fromPartial(c);
        newConfig.server = value;
        await setConfig(newConfig);

        props.postSubmit?.();
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
        <Card>
          <CardHeader>
            <h2>Server Settings</h2>
          </CardHeader>

          <CardContent class="flex flex-col gap-4">
            <div>
              <form.Field
                name="applicationName"
                validators={notEmptyValidator()}
              >
                {buildTextFormField({
                  label: () => <div class={labelWidth}>App Name</div>,
                  info: (
                    <p>
                      The name of your application. Used e.g. in emails sent to
                      users.
                    </p>
                  ),
                })}
              </form.Field>
            </div>

            <div>
              <form.Field name="siteUrl" validators={notEmptyValidator()}>
                {buildTextFormField({
                  label: () => <div class={labelWidth}>Site URL</div>,
                  info: (
                    <p>
                      The public address under which the server is reachable.
                      Used e.g. for auth, e.g. verification links sent via
                      Email.
                    </p>
                  ),
                })}
              </form.Field>
            </div>

            <div>
              <form.Field name="logsRetentionSec">
                {buildNumberFormField({
                  integer: true,
                  label: () => (
                    <div class={labelWidth}>Log Retention (sec)</div>
                  ),
                  info: (
                    <p>
                      A background task periodically cleans up logs older than
                      above retention period. Setting the retention to zero
                      turns off the cleanup and logs will be retained
                      indefinitely.
                    </p>
                  ),
                })}
              </form.Field>
            </div>
          </CardContent>
        </Card>

        <div class="flex justify-end pt-4">
          <form.Subscribe
            selector={(state) => ({
              canSubmit: state.canSubmit,
              isSubmitting: state.isSubmitting,
            })}
          >
            {(state) => {
              return (
                <Button
                  type="submit"
                  disabled={!state().canSubmit}
                  variant="default"
                >
                  {state().isSubmitting ? "..." : "Submit"}
                </Button>
              );
            }}
          </form.Subscribe>
        </div>
      </form>
    );
  };

  const serverConfig = () => {
    const c = config.data?.config?.server;
    if (c) {
      // "deep-copy"
      return ServerConfig.decode(ServerConfig.encode(c).finish());
    }
    // Fallback
    return ServerConfig.create();
  };

  const [info] = createResource(async (): Promise<InfoResponse> => {
    const response = await adminFetch("/info");
    return await response.json();
  });

  const width = "w-40";

  return (
    <div class="flex flex-col gap-4">
      <Card>
        <CardHeader>
          <h2>Server Info</h2>
        </CardHeader>

        <CardContent class="flex flex-col gap-4">
          <Switch>
            <Match when={info.error}>info.error</Match>
            <Match when={info.loading}>Loading...</Match>
            <Match when={info()}>
              <TextField class="w-full">
                <div
                  class={`grid items-center ${gapStyle}`}
                  style="grid-template-columns: auto 1fr"
                >
                  <TextFieldLabel class={width}>CPU Threads:</TextFieldLabel>
                  <span>{info()?.threads}</span>

                  <TextFieldLabel class={width}>Compiler:</TextFieldLabel>
                  <span>{info()?.compiler}</span>

                  <TextFieldLabel class={width}>Commit Hash:</TextFieldLabel>
                  <span>{info()?.commit_hash}</span>

                  <TextFieldLabel class={width}>Commit Date:</TextFieldLabel>
                  <span>{info()?.commit_date}</span>
                </div>
              </TextField>
            </Match>
          </Switch>
        </CardContent>
      </Card>

      <Show when={config.isError}>Failed to fetch config</Show>

      <Show when={config.isLoading}>Loading</Show>

      <Show when={config.isSuccess}>
        <Form config={serverConfig()} />
      </Show>

      {import.meta.env.DEV && (
        <div class="mt-4 flex justify-end">
          <Button
            variant={"default"}
            onClick={() => {
              throw new Date().toLocaleString();
            }}
          >
            Throw Error
          </Button>
        </div>
      )}
    </div>
  );
}

function BackupImportSettings(props: CommonProps) {
  const config = createConfigQuery();

  const Form = (p: { config: ServerConfig }) => {
    const form = createForm<ServerConfig>(() => ({
      defaultValues: p.config,
      onSubmit: async ({ value }: { value: ServerConfig }) => {
        const c = config.data?.config;
        if (!c) {
          console.warn("Missing base config:");
          return;
        }

        const newConfig = Config.fromPartial(c);
        newConfig.server = value;
        await setConfig(newConfig);
      },
    }));

    form.useStore((state) => {
      if (state.isDirty) {
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
        <div class="flex flex-col gap-4">
          <Card>
            <CardHeader>
              <h2>Backup Settings</h2>
            </CardHeader>

            <CardContent class="flex flex-col gap-1">
              <form.Field name="backupIntervalSec">
                {buildNumberFormField({
                  integer: true,
                  label: () => (
                    <div class={labelWidth}>
                      <Label>Backup interval (s)</Label>
                    </div>
                  ),
                  info: backupInfo,
                })}
              </form.Field>
            </CardContent>
          </Card>

          <Card class="text-sm">
            <CardHeader>
              <h2>Data Import {"&"} Export</h2>
            </CardHeader>

            <CardContent>
              <p class="mt-2">
                Data import and export from and to Sql via the UI is not yet
                supported, however with TrailBase not relying on specific
                metadata you can use all the usual suspects around sqlite and
                the data will show up in the table editor. If you import your
                data into a table with strict typing and an UUIDv7 primary
                column you'll also be able to expose the data via restful APIs.
              </p>

              <p class="my-2">Import, e.g.:</p>
              <pre class="whitespace-pre-wrap ml-4">
                $ cat dump.sql | sqlite3 main.db
              </pre>

              <p class="my-2">Output, e.g.:</p>

              <pre class="whitespace-pre-wrap ml-4">
                $ sqlite3 main.db
                <br />
                sqlite&gt; .output dump.db
                <br />
                sqlite&gt; .dump
                <br />
              </pre>
            </CardContent>
          </Card>

          <div class="flex justify-end pt-4">
            <form.Subscribe
              selector={(state) => ({
                canSubmit: state.canSubmit,
                isSubmitting: state.isSubmitting,
              })}
            >
              {(state) => {
                return (
                  <Button
                    type="submit"
                    disabled={!state().canSubmit}
                    variant="default"
                  >
                    {state().isSubmitting ? "..." : "Submit"}
                  </Button>
                );
              }}
            </form.Subscribe>
          </div>
        </div>
      </form>
    );
  };

  const serverConfig = () => {
    const c = config.data?.config?.server;
    if (c) {
      // "deep-copy"
      return ServerConfig.decode(ServerConfig.encode(c).finish());
    }
    // Fallback
    return ServerConfig.create();
  };

  return (
    <>
      <Show when={config.isError}>Failed to fetch config</Show>

      <Show when={config.isLoading}>Loading</Show>

      <Show when={config.isSuccess}>
        <Form config={serverConfig()} />
      </Show>
    </>
  );
}

function WrapSidebar<T>(
  base: string,
  route: string,
  site: Site,
): Component<RouteSectionProps<T>> {
  const [dirty, setDirty] = createSignal(false);

  return (_props: RouteSectionProps) => {
    function First(props: { horizontal: boolean }) {
      const navigate = useNavigate();

      const flexStyle = props.horizontal ? "flex flex-col" : "flex";

      return (
        <div class={`${flexStyle} gap-2 p-4`}>
          <For each={Object.entries(sites)}>
            {([r, s]) => {
              const [dialogOpen, setDialogOpen] = createSignal(false);

              return (
                <Dialog
                  id="confirm"
                  modal={true}
                  open={dialogOpen()}
                  onOpenChange={setDialogOpen}
                >
                  <ConfirmCloseDialog
                    back={() => setDialogOpen(false)}
                    confirm={() => {
                      setDialogOpen(false);
                      navigate(base + r);
                    }}
                  />

                  <Button
                    class="text-nowrap"
                    variant={route === r ? "default" : "outline"}
                    onClick={() => {
                      if (route !== r) {
                        if (!dirty()) {
                          navigate(base + r);
                          return;
                        }

                        setDialogOpen(true);
                      }
                    }}
                  >
                    {s.label}
                  </Button>
                </Dialog>
              );
            }}
          </For>
        </div>
      );
    }

    function Second() {
      return (
        <div class="grow overflow-x-hidden">
          <h1 class="flex gap-4 m-4">
            <span class="text-accent-600">Settings</span>
            <span class="text-accent-600">&gt;</span>
            <span>{site.label}</span>
          </h1>

          <Separator />

          <div class="m-4">
            <site.child
              markDirty={() => setDirty(true)}
              postSubmit={() => {
                setDirty(false);
                showToast({
                  title: "submitted",
                  variant: "success",
                });
              }}
            />
          </div>
        </div>
      );
    }

    return <SplitView first={First} second={Second} />;
  };
}

interface CommonProps {
  markDirty: () => void;
  postSubmit: () => void;
}

interface Site {
  label: string;
  child: Component<CommonProps>;
}

const sites: { [k: string]: Site } = {
  "/": {
    label: "Host",
    child: ServerSettings,
  },
  "/email": {
    label: "E-mail",
    child: EmailSettings,
  },
  "/auth": {
    label: "Auth",
    child: AuthSettings,
  },
  "/schema": {
    label: "Schemas",
    child: SchemaSettings,
  },
  "/backup": {
    label: "Backup",
    child: BackupImportSettings,
  },
} as const;

export function SettingsPages() {
  return (
    <>
      <For each={Object.entries(sites)}>
        {([route, site]) => (
          <Route
            path={route}
            component={WrapSidebar("/settings", route, site)}
          />
        )}
      </For>
    </>
  );
}

const backupInfo: JSXElement = (
  <p class="text-sm">
    Setting the backup interval to zero will disable periodic backups on next
    server start. Backups will lock the database for the duration of the backup,
    which is typically fine for small data sets. However, we recommend a more
    continuous disaster recovery solution such as{" "}
    <a href="https://litestream.io/">Litestream</a> to avoid locking and avoid
    losing changes made between backups.
  </p>
);

const labelWidth = "w-40";
