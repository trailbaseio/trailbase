import {
  createSignal,
  onMount,
  onCleanup,
  Show,
  Switch,
  Match,
} from "solid-js";
import { createForm } from "@tanstack/solid-form";
import { useQueryClient } from "@tanstack/solid-query";
import type { InfoResponse } from "trailbase-bindings";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { TextField, TextFieldLabel } from "@/components/ui/text-field";

import {
  notEmptyValidator,
  unsetOrValidUrl,
  buildOptionalIntegerFormField,
  buildTextFormField,
  buildOptionalTextFormField,
  gapStyle,
} from "@/components/FormFields";
import { Version } from "@/components/Version";
import type { CommonProps } from "@/components/settings/SettingsPage";

import {
  Config,
  ServerConfig,
  ProxyClientIpHeader,
  proxyClientIpHeaderToJSON,
} from "@proto/config";
import { createConfigQuery, setConfig } from "@/lib/api/config";
import { createSystemInfoQuery } from "@/lib/api/info";

export function ServerSettings(props: CommonProps) {
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
      await setConfig({
        client: queryClient,
        config: newConfig,
        throw: true,
      });

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
      class="flex flex-col gap-4"
    >
      <Card>
        <CardHeader>
          <h2>Settings</h2>
        </CardHeader>

        <CardContent class="flex flex-col gap-4">
          <form.Field name="applicationName" validators={notEmptyValidator()}>
            {buildTextFormField({
              label: () => <div class={labelWidth}>App Name</div>,
              info: (
                <p>
                  The name of your application, e.g. used in mails sent to users
                  when signing up.
                </p>
              ),
            })}
          </form.Field>

          <div>
            <form.Field name="siteUrl" validators={unsetOrValidUrl()}>
              {buildOptionalTextFormField({
                label: () => <div class={labelWidth}>Site URL</div>,
                placeholder: "https://trailbase.io",
                info: (
                  <p>
                    The public URL of your server, e.g. used for auth redirects,
                    email verification links.
                  </p>
                ),
              })}
            </form.Field>
          </div>

          <div>
            <form.Field name="logsRetentionSec">
              {buildOptionalIntegerFormField({
                label: () => <div class={labelWidth}>Log Retention (sec)</div>,
                info: (
                  <p>
                    A background job periodically cleans up logs older than the
                    above retention period. Setting the retention to zero turns
                    off the cleanup retaining logs indefinitely.
                  </p>
                ),
              })}
            </form.Field>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <h2>Proxy</h2>
        </CardHeader>

        <CardContent>
          <p class="mb-4 text-sm">
            If your TrailBase instance sits behind (reverse) proxies, the peer
            address will be the proxy's. In order for traffic attribution
            (logging & rate limiting) to work correctly, proxies will attach a
            header forwarding their respective peer's address. Specify your
            proxy setup to narrowly extract a specific header controlled by your
            proxy (as opposed to clients).
          </p>

          <form.Field name="proxy.clientIpHeader">
            {(field) => {
              return (
                <TextField class="w-full">
                  <div
                    class="grid items-center gap-x-2 gap-y-1"
                    style={{ "grid-template-columns": "auto 1fr" }}
                  >
                    <div class="w-40">
                      <TextFieldLabel>Client-IP Header</TextFieldLabel>
                    </div>

                    <div class="w-full">
                      <ProxyHeaderSelect
                        value={field().state.value}
                        handleChange={field().handleChange}
                      />
                    </div>
                  </div>
                </TextField>
              );
            }}
          </form.Field>
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
    </form>
  );
}

function ProxyHeaderSelect(props: {
  value: ProxyClientIpHeader | undefined;
  handleChange: (v: ProxyClientIpHeader) => void;
  disabled?: boolean;
}) {
  return (
    <Select<ProxyClientIpHeader | undefined>
      value={props.value}
      disabled={props.disabled}
      options={proxyClientIpHeaderOptions()}
      placeholder={proxyClientIpHeaderLabel(undefined)}
      itemComponent={(props) => (
        <SelectItem item={props.item}>
          {proxyClientIpHeaderLabel(props.item.rawValue)}
        </SelectItem>
      )}
      onChange={(value) => {
        props.handleChange(
          value ?? ProxyClientIpHeader.PROXY_CLIENT_IP_HEADER_UNDEFINED,
        );
      }}
    >
      <SelectTrigger>
        <SelectValue<ProxyClientIpHeader>>
          {(_state) => proxyClientIpHeaderLabel(props.value)}
        </SelectValue>
      </SelectTrigger>

      <SelectContent />
    </Select>
  );
}

function proxyClientIpHeaderOptions(): (ProxyClientIpHeader | undefined)[] {
  return [
    undefined,
    // ProxyClientIpHeader.PROXY_CLIENT_IP_HEADER_UNDEFINED,
    ProxyClientIpHeader.NONE,
    ProxyClientIpHeader.X_FORWARDED_FOR,
    ProxyClientIpHeader.X_REAL_IP,
    ProxyClientIpHeader.CF_CONNECTING_IP,
    ProxyClientIpHeader.TRUE_CLIENT_IP,
    ProxyClientIpHeader.FLY_CLIENT_IP,
    ProxyClientIpHeader.CLOUDFRONT_VIEWER_ADDRESS,
  ];
}

function proxyClientIpHeaderLabel(
  header: ProxyClientIpHeader | undefined,
): string {
  switch (header) {
    case ProxyClientIpHeader.NONE:
      return "None (no proxy)";
    case ProxyClientIpHeader.X_FORWARDED_FOR:
      return "X-Forwarded-For";
    case ProxyClientIpHeader.X_REAL_IP:
      return "X-Real-Ip";
    case ProxyClientIpHeader.TRUE_CLIENT_IP:
      return "True-Client-Ip (e.g. Cloudflare enterprise)";
    case ProxyClientIpHeader.CF_CONNECTING_IP:
      return "Cloudflare (Cf-Connecting-Ip)";
    case ProxyClientIpHeader.FLY_CLIENT_IP:
      return "Fly";
    case ProxyClientIpHeader.CLOUDFRONT_VIEWER_ADDRESS:
      return "AWS CloudFront";
    case ProxyClientIpHeader.PROXY_CLIENT_IP_HEADER_UNDEFINED:
    case undefined:
      return "Any Header (insecure default)";
    default:
      return proxyClientIpHeaderToJSON(header);
  }
}

function SystemInformation(props: { systemInfo: InfoResponse }) {
  const info = () => props.systemInfo;

  const calcUptime = (): number => {
    const now: number = Date.now() / 1000;
    return now - Number(info().start_time);
  };

  // Running second timer
  const [uptime, setUptime] = createSignal(calcUptime());
  let handle: ReturnType<typeof setTimeout> | undefined = undefined;
  onMount(() => {
    if (handle) {
      clearInterval(handle);
    }
    handle = setInterval(() => setUptime(calcUptime()), 1000);
  });

  onCleanup(() => {
    if (handle) {
      clearInterval(handle);
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

        <Show when={props.systemInfo.postgres}>
          <TextFieldLabel class={width}>Postgres:</TextFieldLabel>
          <span>enabled</span>
        </Show>
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

  return new Intl.DurationFormat("en").format({
    days,
    hours,
    minutes,
    seconds: remainingSeconds,
  });
}

const labelWidth = "w-40";
