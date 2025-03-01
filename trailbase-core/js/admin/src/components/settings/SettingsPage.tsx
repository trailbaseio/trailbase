import {
  createResource,
  createSignal,
  For,
  Show,
  Switch,
  Match,
} from "solid-js";
import type { Component, JSXElement, Signal } from "solid-js";
import { useParams, useNavigate } from "@solidjs/router";
import { createForm } from "@tanstack/solid-form";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { Dialog } from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import { TextField, TextFieldLabel } from "@/components/ui/text-field";
import { showToast } from "@/components/ui/toast";

import { Config, ServerConfig } from "@proto/config";
import {
  notEmptyValidator,
  buildOptionalNumberFormField,
  buildTextFormField,
  gapStyle,
} from "@/components/FormFields";
import { Header } from "@/components/Header";
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
                {buildOptionalNumberFormField({
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
    return ServerConfig.fromJSON({});
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
                  style={{ "grid-template-columns": "auto 1fr" }}
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

        props.postSubmit();
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
                {buildOptionalNumberFormField({
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
              <pre class="ml-4 whitespace-pre-wrap">
                $ cat dump.sql | sqlite3 main.db
              </pre>

              <p class="my-2">Output, e.g.:</p>

              <pre class="ml-4 whitespace-pre-wrap">
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
    return ServerConfig.fromJSON({});
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

function Sidebar(props: {
  activeRoute: string | undefined;
  horizontal: boolean;
  dirty: Signal<boolean>;
}) {
  const navigate = useNavigate();
  const [dirty, setDirty] = props.dirty;

  return (
    <div class={`${props.horizontal ? "flex flex-col" : "flex"} gap-2 p-4`}>
      <For each={sites}>
        {(s: Site) => {
          const [dialogOpen, setDialogOpen] = createSignal(false);
          const match = () => props.activeRoute === s.route;

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
                  setDirty(false);
                  navigate("/settings/" + s.route);
                }}
              />

              <Button
                class="text-nowrap"
                variant={match() ? "default" : "outline"}
                onClick={() => {
                  if (!match()) {
                    if (!dirty()) {
                      navigate("/settings/" + s.route);
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

interface CommonProps {
  markDirty: () => void;
  postSubmit: () => void;
}

interface Site {
  route: string;
  label: string;
  child: Component<CommonProps>;
}

const sites = [
  {
    route: "host",
    label: "Host",
    child: ServerSettings,
  },
  {
    route: "email",
    label: "E-mail",
    child: EmailSettings,
  },
  {
    route: "auth",
    label: "Auth",
    child: AuthSettings,
  },
  {
    route: "schema",
    label: "Schemas",
    child: SchemaSettings,
  },
  {
    route: "backup",
    label: "Backup",
    child: BackupImportSettings,
  },
] as const;

export function SettingsPage() {
  const params = useParams<{ group: string }>();
  const [dirty, setDirty] = createSignal(false);

  const activeSite = () => {
    const g = params?.group;
    if (g) {
      return sites.find((s) => s.route == g) ?? sites[0];
    }
    return sites[0];
  };

  const First = (props: { horizontal: boolean }) => (
    <Sidebar
      horizontal={props.horizontal}
      activeRoute={activeSite().route}
      dirty={[dirty, setDirty]}
    />
  );

  function Second() {
    const p = () =>
      ({
        markDirty: () => setDirty(true),
        postSubmit: () => {
          setDirty(false);
          showToast({
            title: "submitted",
            variant: "success",
          });
        },
      }) as CommonProps;

    return (
      <>
        <Header title="Settings" titleSelect={activeSite().label} />

        <div class="m-4">{activeSite().child(p())}</div>
      </>
    );
  }

  return <SplitView first={First} second={Second} />;
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
