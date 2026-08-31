import { persistentAtom } from "@nanostores/persistent";
import {
  createSignal,
  onMount,
  onCleanup,
  For,
  Show,
  Switch,
  Match,
  createEffect,
  untrack,
} from "solid-js";
import type { Component, JSX } from "solid-js";
import { useParams, useNavigate } from "@solidjs/router";
import { createForm } from "@tanstack/solid-form";
import {
  TbOutlineBriefcase,
  TbOutlineDatabaseExport,
  TbOutlineDeviceFloppy,
  TbOutlineMail,
  TbOutlineRefresh,
  TbOutlineServer,
  TbOutlineTable,
  TbOutlineUser,
} from "solid-icons/tb";
import { IconProps } from "solid-icons";
import { useQueryClient } from "@tanstack/solid-query";

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
import { BackupSettings } from "@/components/settings/BackupSettings";
import { IconButton } from "@/components/IconButton";
import { Version } from "@/components/Version";
import { SettingsFormActions } from "@/components/settings/SettingsFormActions";
import { Callout, CalloutContent, CalloutTitle } from "@/components/ui/callout";

import {
  createConfigQuery,
  setConfig,
  invalidateConfig,
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
              <Callout variant="error" role="alert">
                <CalloutTitle>Unable to load runtime information</CalloutTitle>
                <CalloutContent>Please try again later.</CalloutContent>
              </Callout>
            </Match>

            <Match when={systemInfo.isLoading}>
              <div role="status">Loading runtime information...</div>
            </Match>

            <Match when={systemInfo.isSuccess}>
              <SystemInformation systemInfo={systemInfo.data!} />
            </Match>
          </Switch>
        </CardContent>
      </Card>

      <Switch>
        <Match when={config.isError}>
          <Callout variant="error" role="alert">
            <CalloutTitle>Unable to load settings</CalloutTitle>
            <CalloutContent>Please try again later.</CalloutContent>
          </Callout>
        </Match>

        <Match when={config.isLoading}>
          <div role="status">Loading settings...</div>
        </Match>

        <Match when={config.data?.config}>
          <ServerSettingsForm config={config.data!.config!} {...props} />
        </Match>
      </Switch>
    </div>
  );
}

function cloneServerConfig(config: ServerConfig) {
  return ServerConfig.decode(ServerConfig.encode(config).finish());
}

function sameServerConfig(left: ServerConfig, right: ServerConfig) {
  const leftBytes = ServerConfig.encode(left).finish();
  const rightBytes = ServerConfig.encode(right).finish();
  return (
    leftBytes.length === rightBytes.length &&
    leftBytes.every((byte, index) => byte === rightBytes[index])
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
    return server ? cloneServerConfig(server) : ServerConfig.fromJSON({});
  }

  const initialValues = serverConfig(props.config);
  const [savedValues, setSavedValues] = createSignal(initialValues);
  let editBaseline = cloneServerConfig(initialValues);
  const [submitError, setSubmitError] = createSignal(false);
  let lastIncoming = cloneServerConfig(initialValues);
  let active = true;
  onCleanup(() => {
    active = false;
  });
  const form = createForm(() => ({
    defaultValues: cloneServerConfig(savedValues()),
    onSubmit: async ({ value }: { value: ServerConfig }) => {
      setSubmitError(false);
      const latest = cloneServerConfig(savedValues());
      if (value.applicationName !== editBaseline.applicationName) {
        latest.applicationName = value.applicationName;
      }
      if (value.siteUrl !== editBaseline.siteUrl) {
        latest.siteUrl = value.siteUrl;
      }
      if (value.logsRetentionSec !== editBaseline.logsRetentionSec) {
        latest.logsRetentionSec = value.logsRetentionSec;
      }
      const newConfig = Config.fromPartial(props.config);
      newConfig.server = latest;
      try {
        await setConfig({
          client: queryClient,
          config: newConfig,
          throw: true,
        });
        if (!active) return;

        const saved = cloneServerConfig(newConfig.server!);
        setSavedValues(saved);
        editBaseline = cloneServerConfig(saved);
        const stillModified = !sameServerConfig(saved, formValues());
        if (!stillModified) form.reset(cloneServerConfig(saved));
        props.postSubmit?.(stillModified);
      } catch {
        if (active) setSubmitError(true);
      }
    },
  }));

  const formValues = form.useSelector((state) => state.values);
  const modified = () => {
    const current = formValues();
    return (
      current.applicationName !== editBaseline.applicationName ||
      current.siteUrl !== editBaseline.siteUrl ||
      current.logsRetentionSec !== editBaseline.logsRetentionSec
    );
  };

  createEffect(() => {
    const incoming = serverConfig(props.config);
    if (sameServerConfig(lastIncoming, incoming)) return;

    lastIncoming = cloneServerConfig(incoming);
    const wasModified = untrack(modified);
    setSavedValues(cloneServerConfig(incoming));
    if (!wasModified) {
      editBaseline = cloneServerConfig(incoming);
      form.reset(cloneServerConfig(incoming));
    }
  });
  createEffect(() => props.setDirty(modified()));

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

        <form.Subscribe
          selector={(state) => ({
            canSubmit: state.canSubmit,
            isSubmitting: state.isSubmitting,
          })}
        >
          {(state) => (
            <>
              <Show when={submitError()}>
                <Callout variant="error" role="alert">
                  <CalloutTitle>Unable to save settings</CalloutTitle>
                  <CalloutContent>
                    Check your values and try again.
                  </CalloutContent>
                </Callout>
              </Show>
              <SettingsFormActions
                dirty={modified()}
                canSubmit={state().canSubmit}
                isSubmitting={state().isSubmitting}
                onReset={() => {
                  setSubmitError(false);
                  editBaseline = cloneServerConfig(savedValues());
                  form.reset(cloneServerConfig(savedValues()));
                }}
              />
            </>
          )}
        </form.Subscribe>
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
  let handle: ReturnType<typeof setInterval> | undefined = undefined;
  onMount(() => {
    if (handle !== undefined) {
      clearInterval(handle);
    }
    handle = setInterval(() => setUptime(calcUptime()), 1000);
  });

  onCleanup(() => {
    if (handle !== undefined) {
      clearInterval(handle);
    }
    handle = undefined;
  });

  const width = "w-40";
  return (
    <dl
      class={`grid items-center ${gapStyle}`}
      style={{ "grid-template-columns": "auto 1fr" }}
    >
      <dt class={width}>CPU Threads:</dt>
      <dd>{info().threads}</dd>

      <dt class={width}>Compiler:</dt>
      <dd>{info().compiler}</dd>

      <dt class={width}>Commit Hash:</dt>
      <dd>
        <a
          href={`https://github.com/trailbaseio/trailbase/commit/${info().commit_hash}`}
        >
          {info().commit_hash?.substring(0, 10)}
        </a>
      </dd>

      <dt class={width}>Commit Date:</dt>
      <dd>{info().commit_date}</dd>

      <dt class={width}>Version:</dt>
      <dd>
        <Version info={info()} />
      </dd>

      <dt class={width}>Uptime:</dt>
      <dd>{formatDuration(uptime())}</dd>

      <dt class={width}>Arguments:</dt>
      <dd class="font-mono">{info().command_line_arguments?.join(" ")}</dd>

      <Show when={props.systemInfo.postgres}>
        <dt class={width}>Postgres:</dt>
        <dd>enabled</dd>
      </Show>
    </dl>
  );
}

function formatDuration(seconds: number): string {
  const days = Math.floor(seconds / (24 * 3600));
  seconds %= 24 * 3600;

  const hours = Math.floor(seconds / 3600);
  seconds %= 3600;

  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = Math.floor(seconds % 60);

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
  setDirty: (dirty: boolean) => void;
  postSubmit: (dirty?: boolean) => void;
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
    label: "General",
    child: ServerSettings,
    icon: TbOutlineServer,
  },
  {
    route: "email",
    label: "Email",
    child: EmailSettings,
    icon: TbOutlineMail,
  },
  {
    route: "auth",
    label: "Authentication",
    child: AuthSettings,
    icon: TbOutlineUser,
  },
  {
    route: "backup",
    label: "Backups",
    child: BackupSettings,
    icon: TbOutlineDeviceFloppy,
  },
  {
    route: "jobs",
    label: "Jobs",
    child: JobSettings,
    icon: TbOutlineBriefcase,
  },
  {
    route: "data",
    label: "Databases",
    child: DatabaseSettings,
    icon: TbOutlineDatabaseExport,
  },
  {
    route: "schema",
    label: "Schemas",
    child: SchemaSettings,
    icon: TbOutlineTable,
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

    const index = $uiState.get().selected;
    return sites[index === undefined || index >= sites.length ? 0 : index];
  };

  createEffect(() => {
    const g = params?.group;
    if (!g) {
      return;
    }

    const index = sites.findIndex((s) => s.route == g);
    if (index >= 0) {
      $uiState.set({
        ...$uiState.get(),
        selected: index,
      });
    }
  });

  const p = () =>
    ({
      setDirty,
      postSubmit: (dirty = false) => {
        setDirty(dirty);
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
          <IconButton
            aria-label="Refresh settings"
            onClick={() => invalidateConfig(queryClient)}
          >
            <TbOutlineRefresh />
          </IconButton>
        }
      />

      <div class="m-4 max-w-5xl">{activeSite().child(p())}</div>
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

      <SidebarInset>
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

type SettingsUiState = {
  selected?: number;
};

const $uiState = persistentAtom<SettingsUiState>(
  "settings:state",
  {},
  {
    encode: JSON.stringify,
    decode: JSON.parse,
  },
);

const labelWidth = "w-40";
