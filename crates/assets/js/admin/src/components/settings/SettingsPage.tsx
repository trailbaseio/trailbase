import {
  createSignal,
  onMount,
  onCleanup,
  For,
  Show,
  Switch,
  Match,
} from "solid-js";
import type { Component, JSX } from "solid-js";
import { useParams, useNavigate } from "@solidjs/router";
import { createForm } from "@tanstack/solid-form";
import {
  TbRefresh,
  TbMail,
  TbServer,
  TbUser,
  TbBriefcase,
  TbTable,
  TbDatabaseExport,
} from "solid-icons/tb";
import { IconProps } from "solid-icons";
import { useQueryClient } from "@tanstack/solid-query";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { Dialog } from "@/components/ui/dialog";
import { showToast } from "@/components/ui/toast";
import {
  useSidebar,
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupContent,
  SidebarInset,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarRail,
  SidebarTrigger,
} from "@/components/ui/sidebar";
import { TextField, TextFieldLabel } from "@/components/ui/text-field";

import type { InfoResponse } from "@bindings/InfoResponse";
import { Config, ServerConfig } from "@proto/config";
import {
  notEmptyValidator,
  unsetOrValidUrl,
  buildOptionalIntegerFormField,
  buildTextFormField,
  buildOptionalTextFormField,
  gapStyle,
} from "@/components/FormFields";
import { Header } from "@/components/Header";
import { ConfirmCloseDialog } from "@/components/SafeSheet";
import { AuthSettings } from "@/components/settings/AuthSettings";
import { DatabaseSettings } from "@/components/settings/DatabaseSettings";
import { SchemaSettings } from "@/components/settings/SchemaSettings";
import { EmailSettings } from "@/components/settings/EmailSettings";
import { JobSettings } from "@/components/settings/JobSettings";
import { IconButton } from "@/components/IconButton";
import { Version } from "@/components/Version";

import {
  createConfigQuery,
  setConfig,
  invalidateAllAdminQueries,
} from "@/lib/api/config";
import { createSystemInfoQuery } from "@/lib/api/info";
import { createIsMobile } from "@/lib/signals";

function ServerSettings(props: CommonProps) {
  const config = createConfigQuery();
  const systemInfo = createSystemInfoQuery();

  return (
    <div class="flex flex-col gap-4">
      <Card>
        <CardHeader>
          <h2>Info</h2>
        </CardHeader>

        <CardContent class="flex flex-col gap-4">
          <Switch>
            <Match when={systemInfo.isError}>
              {systemInfo.error?.toString()}
            </Match>

            <Match when={systemInfo.isLoading}>Loading...</Match>

            <Match when={systemInfo.isSuccess}>
              <SystemInformation systemInfo={systemInfo.data!} />
            </Match>
          </Switch>
        </CardContent>
      </Card>

      <Switch>
        <Match when={config.isError}>{config.error?.toString()}</Match>

        <Match when={config.isLoading}>Lading...</Match>

        <Match when={config.data?.config}>
          <ServerSettingsForm config={config.data!.config!} {...props} />
        </Match>
      </Switch>

      <Show when={import.meta.env.DEV}>
        <div class="flex justify-end gap-4">
          <Button
            variant={"destructive"}
            type="button"
            onClick={() => {
              throw new Error("test sync exception");
            }}
          >
            DEV: Throw
          </Button>

          <Button
            variant={"destructive"}
            type="button"
            onClick={() => {
              (async () => {
                throw new Error("test async exception");
              })();
            }}
          >
            DEV: Async Throw
          </Button>
        </div>
      </Show>
    </div>
  );
}

function ServerSettingsForm(
  props: {
    config: Config;
  } & CommonProps,
) {
  const queryClient = useQueryClient();

  function serverConfig(config: Config) {
    const server = config.server;
    // "deep-copy" & fallback
    return server
      ? ServerConfig.decode(ServerConfig.encode(server).finish())
      : ServerConfig.fromJSON({});
  }

  const form = createForm(() => ({
    defaultValues: serverConfig(props.config),
    onSubmit: async ({ value }: { value: ServerConfig }) => {
      const newConfig = Config.fromPartial(props.config);
      newConfig.server = value;
      await setConfig(queryClient, newConfig);

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
      method="dialog"
      onSubmit={(e: SubmitEvent) => {
        e.preventDefault();
        form.handleSubmit();
      }}
    >
      <div class="flex flex-col gap-4">
        <Card>
          <CardHeader>
            <h2>Settings</h2>
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
                      The name of your application, e.g. used in mails sent to
                      users when signing up.
                    </p>
                  ),
                })}
              </form.Field>
            </div>

            <div>
              <form.Field name="siteUrl" validators={unsetOrValidUrl()}>
                {buildOptionalTextFormField({
                  label: () => <div class={labelWidth}>Site URL</div>,
                  placeholder: "https://trailbase.io",
                  info: (
                    <p>
                      The public URL of your server, e.g. used for auth
                      redirects, email verification links.
                    </p>
                  ),
                })}
              </form.Field>
            </div>

            <div>
              <form.Field name="logsRetentionSec">
                {buildOptionalIntegerFormField({
                  label: () => (
                    <div class={labelWidth}>Log Retention (sec)</div>
                  ),
                  info: (
                    <p>
                      A background job periodically cleans up logs older than
                      the above retention period. Setting the retention to zero
                      turns off the cleanup retaining logs indefinitely.
                    </p>
                  ),
                })}
              </form.Field>
            </div>
          </CardContent>
        </Card>

        <div class="flex justify-end gap-4">
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
}

function SystemInformation(props: { systemInfo: InfoResponse }) {
  const info = () => props.systemInfo;

  const calcUptime = (): number => {
    const now: number = Date.now() / 1000;
    return now - Number(info().start_time);
  };

  // Running second timer
  const [uptime, setUptime] = createSignal(calcUptime());
  let handle: unknown = undefined;
  onMount(() => {
    if (handle) {
      clearInterval(handle as any);
    }
    handle = setInterval(() => setUptime(calcUptime()), 1000);
  });

  onCleanup(() => {
    if (handle) {
      clearInterval(handle as any);
    }
    handle = undefined;
  });

  const width = "w-40";
  return (
    <TextField class="w-full">
      <div
        class={`grid items-center ${gapStyle}`}
        style={{ "grid-template-columns": "auto 1fr" }}
      >
        <TextFieldLabel class={width}>CPU Threads:</TextFieldLabel>
        <span>{info().threads}</span>

        <TextFieldLabel class={width}>Compiler:</TextFieldLabel>
        <span>{info().compiler}</span>

        <TextFieldLabel class={width}>Commit Hash:</TextFieldLabel>
        <span>
          <a
            href={`https://github.com/trailbaseio/trailbase/commit/${info().commit_hash}`}
          >
            {info().commit_hash?.substring(0, 10)}
          </a>
        </span>

        <TextFieldLabel class={width}>Commit Date:</TextFieldLabel>
        <span>{info().commit_date}</span>

        <TextFieldLabel class={width}>Version:</TextFieldLabel>
        <span>
          <Version info={info()} />
        </span>

        <TextFieldLabel class={width}>Uptime:</TextFieldLabel>
        <span>{formatDuration(uptime())}</span>

        <TextFieldLabel class={width}>Arguments:</TextFieldLabel>
        <span class="font-mono">
          {info().command_line_arguments?.join(" ")}
        </span>
      </div>
    </TextField>
  );
}

function formatDuration(seconds: number): string {
  const days = Math.floor(seconds / (24 * 3600));
  seconds %= 24 * 3600;

  const hours = Math.floor(seconds / 3600);
  seconds %= 3600;

  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = Math.floor(seconds % 60);

  // @ts-expect-error Intl.DurationFormat type definitions missing: https://github.com/microsoft/TypeScript/issues/60608
  return new Intl.DurationFormat("en").format({
    days,
    hours,
    minutes,
    seconds: remainingSeconds,
  });
}

type DirtyDialogState = {
  nextRoute: string;
};

function SettingsSidebar(props: {
  activeRoute: string | undefined;
  dirty: boolean;
  openDirtyDialog: (s: DirtyDialogState) => void;
}) {
  const { setOpenMobile } = useSidebar();
  const navigate = useNavigate();

  return (
    <div class="p-2">
      <SidebarGroupContent>
        <SidebarMenu>
          <For each={sites}>
            {(s: Site) => {
              const match = () => props.activeRoute === s.route;

              return (
                <SidebarMenuItem>
                  <SidebarMenuButton
                    isActive={match()}
                    size="md"
                    variant="default"
                    onClick={() => {
                      setOpenMobile(false);
                      if (match()) {
                        // Nothing to do.
                        return;
                      }

                      if (!props.dirty) {
                        navigate("/settings/" + s.route);
                        return;
                      }

                      // Open a dirty warning.
                      props.openDirtyDialog({
                        nextRoute: s.route,
                      });
                    }}
                  >
                    {<s.icon />}

                    {s.label}
                  </SidebarMenuButton>
                </SidebarMenuItem>
              );
            }}
          </For>
        </SidebarMenu>
      </SidebarGroupContent>
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
  icon: (props: IconProps) => JSX.Element;
}

const sites = [
  {
    route: "host",
    label: "Host",
    child: ServerSettings,
    icon: TbServer,
  },
  {
    route: "email",
    label: "Email",
    child: EmailSettings,
    icon: TbMail,
  },
  {
    route: "auth",
    label: "Auth",
    child: AuthSettings,
    icon: TbUser,
  },
  {
    route: "jobs",
    label: "Jobs",
    child: JobSettings,
    icon: TbBriefcase,
  },
  {
    route: "data",
    label: "Databases",
    child: DatabaseSettings,
    icon: TbDatabaseExport,
  },
  {
    route: "schema",
    label: "Schemas",
    child: SchemaSettings,
    icon: TbTable,
  },
] as const;

export function SettingsPage() {
  const queryClient = useQueryClient();
  const params = useParams<{ group: string }>();
  const navigate = useNavigate();

  const [dirty, setDirty] = createSignal(false);
  const [dirtyDialog, setDirtyDialog] = createSignal<
    DirtyDialogState | undefined
  >();
  const isMobile = createIsMobile();

  const activeSite = () => {
    const g = params?.group;
    if (g) {
      return sites.find((s) => s.route == g) ?? sites[0];
    }
    return sites[0];
  };

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

  const Body = () => (
    <Dialog
      id="switch-settings-dialog"
      open={dirtyDialog() !== undefined}
      onOpenChange={(isOpen) => {
        if (!isOpen) {
          setDirtyDialog();
        }
      }}
      modal={true}
    >
      <ConfirmCloseDialog
        back={() => setDirtyDialog()}
        confirm={() => {
          const state = dirtyDialog();
          if (state) {
            setDirtyDialog();
            setDirty(false);
            navigate("/settings/" + state.nextRoute);
          }
        }}
      />

      <Header
        title="Settings"
        titleSelect={activeSite().label}
        leading={<SidebarTrigger />}
        left={
          <IconButton onClick={() => invalidateAllAdminQueries(queryClient)}>
            <TbRefresh />
          </IconButton>
        }
      />

      <div class="m-4">{activeSite().child(p())}</div>
    </Dialog>
  );

  return (
    <SidebarProvider>
      <Sidebar
        class="absolute"
        variant="sidebar"
        side="left"
        collapsible="offcanvas"
      >
        <SidebarContent>
          <SidebarGroup>
            <SettingsSidebar
              activeRoute={activeSite().route}
              dirty={dirty()}
              openDirtyDialog={setDirtyDialog}
            />
          </SidebarGroup>

          {/* <SidebarFooter /> */}
        </SidebarContent>

        <SidebarRail />
      </Sidebar>

      <SidebarInset class="min-w-0">
        <Switch>
          <Match when={isMobile()}>
            <Body />
          </Match>

          <Match when={!isMobile()}>
            <div class="h-dvh overflow-y-auto">
              <Body />
            </div>
          </Match>
        </Switch>
      </SidebarInset>
    </SidebarProvider>
  );
}

const labelWidth = "w-40";
