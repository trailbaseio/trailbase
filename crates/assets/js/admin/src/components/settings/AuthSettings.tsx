import { createMemo, For, Suspense, Switch, Show, Match, JSX } from "solid-js";
import { useQuery } from "@tanstack/solid-query";
import { createForm } from "@tanstack/solid-form";
import {
  TbArrowBackUp,
  TbCircle,
  TbCircleCheck,
  TbCirclePlus,
  TbTrash,
  TbInfoCircle,
} from "solid-icons/tb";

import {
  buildOptionalIntegerFormField,
  buildOptionalNumberFormField,
  buildOptionalBoolFormField,
  buildOptionalSecretFormField,
  buildOptionalTextFormField,
} from "@/components/FormFields";
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from "@/components/ui/accordion";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";

import {
  AuthConfig,
  Config,
  OAuthProviderConfig,
  OAuthProviderId,
} from "@proto/config";
import { createConfigQuery, setConfig } from "@/lib/api/config";
import { adminFetch } from "@/lib/fetch";
import { createSetOnce } from "@/lib/signals";
import { showSaveFileDialog, copyToClipboard } from "@/lib/utils";

import type { OAuthProviderResponse } from "@bindings/OAuthProviderResponse";
import type { OAuthProviderEntry } from "@bindings/OAuthProviderEntry";

// OAuth2 provider assets.
import openIdConnect from "@shared/assets/oauth2/oidc.svg";
import apple from "@shared/assets/oauth2/apple.svg";
import discord from "@shared/assets/oauth2/discord.svg";
import facebook from "@shared/assets/oauth2/facebook.svg";
import gitlab from "@shared/assets/oauth2/gitlab.svg";
import google from "@shared/assets/oauth2/google.svg";
import microsoft from "@shared/assets/oauth2/microsoft.svg";
import { useQueryClient } from "@tanstack/solid-query";

export const assets = new Map<OAuthProviderId, string>([
  [OAuthProviderId.OIDC0, openIdConnect],
  [OAuthProviderId.APPLE, apple],
  [OAuthProviderId.DISCORD, discord],
  [OAuthProviderId.FACEBOOK, facebook],
  [OAuthProviderId.GITLAB, gitlab],
  [OAuthProviderId.GOOGLE, google],
  [OAuthProviderId.MICROSOFT, microsoft],
]);

// Using a proxy struct for oauth providers, since tanstack only deals with arrays and not maps.
// And rather than trying to hack it an converting on the fly, we're converting
// once upfront from config to proxy and back on submission.
type NamedOAuthProvider = {
  provider: OAuthProviderEntry;
  state?: OAuthProviderConfig;
};
type AuthConfigProxy = Omit<AuthConfig, "oauthProviders"> & {
  namedOAuthProviders: NamedOAuthProvider[];
};

function configToProxy(
  providers: Array<OAuthProviderEntry>,
  config: AuthConfig,
): AuthConfigProxy {
  const idToConfig = new Map<number, OAuthProviderConfig>(
    Object.values(config.oauthProviders).map((c) => {
      const providerId = c.providerId;
      if (!providerId) {
        console.warn("missing provider id:", c);
        return [-1, c];
      }

      return [providerId, c];
    }),
  );

  return {
    ...config,
    namedOAuthProviders: providers.map((provider): NamedOAuthProvider => {
      const config = idToConfig.get(provider.id);
      if (config === undefined) {
        return { provider };
      }

      return {
        provider,
        state: { ...config },
      };
    }),
  };
}

function proxyToConfig(proxy: AuthConfigProxy): AuthConfig {
  const config = AuthConfig.fromPartial({
    ...(proxy as Omit<AuthConfigProxy, "namedOAuthProviders">),
  });
  config.oauthProviders = {};

  for (const entry of proxy.namedOAuthProviders) {
    const p = entry.provider;

    // Only add complete providers back to config, i.e. once that have both a provider id and client secret.
    const clientId = entry.state?.clientId;
    const clientSecret = entry.state?.clientSecret;

    if (clientId && clientSecret) {
      config.oauthProviders[p.name] = {
        providerId: p.id,

        ...entry.state,
      };
    } else {
      console.debug("Skipping incomplete: ", entry);
    }
  }
  return config;
}

function nonEmpty(v: string | undefined): string | undefined {
  return v && v !== "" ? v : undefined;
}

export async function adminListOAuthProviders(): Promise<OAuthProviderResponse> {
  const response = await adminFetch("/oauth_providers", {
    method: "GET",
  });
  return await response.json();
}

function ProviderSettingsSubForm(props: {
  form: ReturnType<typeof createAuthSettingsForm>;
  index: number;
  provider: OAuthProviderEntry;
  siteUrl: string | undefined;
}) {
  const [original, setOnce, { reset }] = createSetOnce<
    OAuthProviderConfig | undefined
  >(undefined);

  const current = createMemo(() =>
    props.form.useStore((state: (typeof props.form)["state"]) => {
      if (state.isSubmitted) {
        reset(state.values.namedOAuthProviders[props.index].state);
      }

      const s = state.values.namedOAuthProviders[props.index].state;
      setOnce({ ...s });
      return s;
    })(),
  );

  const dirty = () => {
    const id = nonEmpty(current()?.clientId) !== nonEmpty(original()?.clientId);
    const secret =
      nonEmpty(current()?.clientSecret) !== nonEmpty(original()?.clientSecret);
    return id || secret;
  };

  const Bullet = () => (
    <Switch fallback={<TbCircle color="grey" />}>
      <Match when={dirty()}>
        <TbCirclePlus color="orange" />
      </Match>

      <Match when={current()?.clientId !== undefined}>
        <TbCircleCheck color="green" />
      </Match>
    </Switch>
  );

  return (
    <AccordionItem value={`item-${props.provider.id}`}>
      <AccordionTrigger>
        <div class="flex items-center gap-4">
          <Bullet />

          <div class="flex items-center gap-2">
            <img
              class="size-[24px]"
              src={assets.get(props.provider.id)}
              alt={props.provider.display_name}
            />
            <span>{props.provider.display_name}</span>
          </div>
        </div>
      </AccordionTrigger>

      <AccordionContent>
        <div class="m-4 flex flex-col gap-2">
          <OAuthCallbackAddressInfo
            provider={props.provider}
            siteUrl={props.siteUrl}
          />

          <props.form.Field
            name={`namedOAuthProviders[${props.index}].state.clientId`}
          >
            {buildOptionalTextFormField({ label: () => <L>Client Id</L> })}
          </props.form.Field>

          <props.form.Field
            name={`namedOAuthProviders[${props.index}].state.clientSecret`}
          >
            {buildOptionalSecretFormField({
              label: () => <L>Client Secret</L>,
              autocomplete: "off",
            })}
          </props.form.Field>

          <Show when={props.provider.id === OAuthProviderId.OIDC0}>
            <props.form.Field
              name={`namedOAuthProviders[${props.index}].state.authUrl`}
            >
              {buildOptionalTextFormField({ label: () => <L>Auth URL</L> })}
            </props.form.Field>

            <props.form.Field
              name={`namedOAuthProviders[${props.index}].state.tokenUrl`}
            >
              {buildOptionalTextFormField({ label: () => <L>Token URL</L> })}
            </props.form.Field>

            <props.form.Field
              name={`namedOAuthProviders[${props.index}].state.userApiUrl`}
            >
              {buildOptionalTextFormField({ label: () => <L>User API URL</L> })}
            </props.form.Field>
          </Show>
        </div>

        <div class="mr-4 flex items-center justify-end gap-2">
          <Button
            variant={"outline"}
            disabled={!dirty()}
            onClick={() => {
              props.form.setFieldValue(
                `namedOAuthProviders[${props.index}].state`,
                original(),
              );
            }}
          >
            <TbArrowBackUp />
          </Button>

          <Button
            variant={"outline"}
            disabled={current()?.clientId === undefined}
            onClick={() => {
              props.form.setFieldValue(
                `namedOAuthProviders[${props.index}].state`,
                undefined,
              );
            }}
          >
            <TbTrash />
          </Button>
        </div>
      </AccordionContent>
    </AccordionItem>
  );
}

function createAuthSettingsForm(opts: {
  config: () => Config;
  values: () => AuthConfigProxy;
  postSubmit: () => void;
}) {
  const queryClient = useQueryClient();

  return createForm(() => {
    return {
      defaultValues: opts.values(),
      onSubmit: async ({ value }) => {
        const newConfig = Config.decode(Config.encode(opts.config()).finish());

        newConfig.auth = proxyToConfig(value);

        console.debug("Submitting provider config:", value);
        await setConfig(queryClient, newConfig);

        opts.postSubmit();
      },
      validators: {
        onChange: ({ value }: { value: AuthConfigProxy }) => {
          // We can return field-level errors from the form-level validation. (we're also not displaying form-level errors right now).
          for (const i in value.namedOAuthProviders) {
            const provider = value.namedOAuthProviders[i];
            const state = provider.state;
            if (state === undefined) {
              continue;
            }

            if (
              state.clientId !== undefined &&
              state.clientSecret === undefined
            ) {
              return {
                form: "invalid data",
                fields: Object.fromEntries([
                  [
                    `namedOAuthProviders[${i}].state.clientSecret`,
                    `Missing client secret for ${provider.provider.display_name}`,
                  ],
                ]),
              };
            }
          }

          return null;
        },
      },
    };
  });
}

function AuthSettingsForm(props: {
  config: Config;
  providers: OAuthProviderResponse;
  markDirty: () => void;
  postSubmit: () => void;
}) {
  const values = createMemo(() =>
    configToProxy(
      props.providers.providers,
      props.config.auth ?? AuthConfig.create(),
    ),
  );

  const form = createAuthSettingsForm({
    config: () => props.config,
    values,
    postSubmit: () => props.postSubmit(),
  });

  form.useStore((state) => {
    if (state.isDirty && !state.isSubmitted) {
      props.markDirty();
    }
  });

  return (
    <form
      method="dialog"
      onSubmit={(e) => {
        e.preventDefault();
        form.handleSubmit();
      }}
    >
      <div class="flex flex-col gap-4">
        <Card>
          <CardHeader>
            <h2>Token Settings</h2>
          </CardHeader>

          <CardContent>
            <div class="flex flex-col gap-4">
              <form.Field name="authTokenTtlSec">
                {buildOptionalIntegerFormField({
                  placeholder: `${60 * 60}`,
                  label: () => (
                    <InfoTooltip label="Auth TTL [sec]">
                      Tokens older than this TTL are considered invalid. A new
                      AuthToken can be minted given a valid refresh Token.
                    </InfoTooltip>
                  ),
                })}
              </form.Field>

              <form.Field name="refreshTokenTtlSec">
                {buildOptionalIntegerFormField({
                  placeholder: `${30 * 24 * 60 * 60}`,
                  label: () => (
                    <InfoTooltip label="Refresh TTL [sec]">
                      Refresh tokens older than this TTL are considered invalid.
                      A refresh token can be renewed by users logging in again.
                    </InfoTooltip>
                  ),
                })}
              </form.Field>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <h2>Password Settings</h2>
          </CardHeader>

          <CardContent>
            <div class="flex flex-col gap-4">
              <form.Field name="disablePasswordAuth">
                {buildOptionalBoolFormField({
                  label: () => (
                    <InfoTooltip label="Disable Registration">
                      When disabled new users will only be able to sign up using
                      OAuth. Existing users can continue to sign in using
                      password-based auth.
                    </InfoTooltip>
                  ),
                })}
              </form.Field>

              <form.Field name="passwordMinimalLength">
                {buildOptionalNumberFormField({
                  integer: true,
                  placeholder: "8",
                  label: () => (
                    <InfoTooltip label="Min Length">
                      Minimal length for new passwords. Does not affect existing
                      registrations unless users choose to change their
                      password.
                    </InfoTooltip>
                  ),
                })}
              </form.Field>

              <form.Field name="passwordMustContainUpperAndLowerCase">
                {buildOptionalBoolFormField({
                  label: () => (
                    <InfoTooltip label="Require Mixed Case">
                      Passwords must contain both, upper and lower case
                      characters. Does not affect existing registrations unless
                      users choose to change their password.
                    </InfoTooltip>
                  ),
                })}
              </form.Field>

              <form.Field name="passwordMustContainDigits">
                {buildOptionalBoolFormField({
                  label: () => (
                    <InfoTooltip label="Require Digits">
                      Passwords must contain digits. Does not affect existing
                      registrations unless users choose to change their
                      password.
                    </InfoTooltip>
                  ),
                })}
              </form.Field>

              <form.Field name="passwordMustContainSpecialCharacters">
                {buildOptionalBoolFormField({
                  label: () => (
                    <InfoTooltip label="Require Special">
                      Passwords must contain special, i.e., non-alphanumeric
                      characters. Does not affect existing registrations unless
                      users choose to change their password.
                    </InfoTooltip>
                  ),
                })}
              </form.Field>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <h2>OAuth Providers</h2>
          </CardHeader>

          <CardContent>
            <form.Field name="namedOAuthProviders">
              {(_field) => {
                return (
                  <Accordion multiple={false} collapsible class="w-full">
                    <For each={values().namedOAuthProviders}>
                      {(provider, index) => {
                        return (
                          <ProviderSettingsSubForm
                            form={form}
                            index={index()}
                            provider={provider.provider}
                            siteUrl={props.config.server?.siteUrl}
                          />
                        );
                      }}
                    </For>
                  </Accordion>
                );
              }}
            </form.Field>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <h2>Public Key</h2>
          </CardHeader>

          <CardContent class="flex flex-col gap-4 text-sm">
            <p>
              TrailBase uses short-lived, stateless JWT Auth tokens and
              asymmetric public/private key cryptography (Ed25519 elliptic
              curves) in combination with longer-lived, stateful refresh tokens.
              Refresh tokens can be trivially exchanged for a fresh short-lived
              auth token for as long as the refresh token has neither expired
              nor been revoked. The main benefit of self-contained, stateless
              auth is that other backend services you may run can simply
              authenticate users by validating a given auth token against the
              public key below w/o having to talk to TrailBase. It's important
              that you keep the corresponding private key secret at all times.
            </p>

            <p>
              A common concern with stateless auth, as opposed to stateful
              session-based auth, is the inability to revoke access in case an
              auth token ever leaks. This is why, Auth tokens are short-lived to
              reduce the impact of any such leak.
            </p>

            <div>
              {/* NOTE: we cannot just have a <a download /> here since admin APIs require CSRF token. */}
              <Button
                variant="default"
                onClick={() => {
                  showSaveFileDialog({
                    contents: async () => {
                      const response = await adminFetch(`/public_key`);
                      return response.body;
                    },
                    filename: "public_key.pep",
                  });
                }}
              >
                Download Public Key
              </Button>
            </div>
          </CardContent>
        </Card>

        <div class="flex justify-end">
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

const providerDashboardUrl: Record<string, string> = {
  discord: "https://discord.com/developers/applications",
  gitlab: "https://gitlab.com",
};

function OAuthCallbackAddressInfo(props: {
  provider: OAuthProviderEntry;
  siteUrl: string | undefined;
}) {
  const address = () => {
    const url = new URL(
      `/api/auth/v1/oauth/${props.provider.name}/callback`,
      props.siteUrl ?? window.location.origin,
    );
    return url.toString();
  };

  const ProviderName = () => {
    const name = props.provider.name;
    const url = providerDashboardUrl[name];

    return (
      <Show when={url} fallback={props.provider.display_name}>
        <a class="underline" href={url}>
          {props.provider.display_name}
        </a>
      </Show>
    );
  };

  return (
    <p class="grow">
      To use this provider, register your application with <ProviderName />{" "}
      using your instance's{" "}
      <Tooltip>
        <TooltipTrigger as="span" onClick={() => copyToClipboard(address())}>
          <span class="underline">Redirect URI</span>{" "}
          <TbInfoCircle class="inline-block" />
        </TooltipTrigger>

        <TooltipContent>
          <span class="font-mono">{address()}</span>
        </TooltipContent>
      </Tooltip>
      . Afterwards fill the credentials issued by {props.provider.display_name}{" "}
      into the fields below.
    </p>
  );
}

export function AuthSettings(props: {
  markDirty: () => void;
  postSubmit: () => void;
}) {
  const providers = useQuery(() => ({
    queryKey: ["admin", "oauthproviders"],
    queryFn: adminListOAuthProviders,
  }));
  const config = createConfigQuery();

  const protoConfig = () => {
    const c = config.data?.config;
    if (c) {
      // "deep-copy"
      return Config.decode(Config.encode(c).finish());
    }
    // Fallback
    return Config.fromJSON({});
  };

  return (
    <Suspense fallback={<div>Loading...</div>}>
      <Switch>
        <Match when={providers.error}>
          <span>Error: {providers.error?.toString()}</span>
        </Match>

        <Match when={config.isError}>
          <span>Error: {config.error?.toString()}</span>
        </Match>

        <Match when={config.data && providers.data}>
          <AuthSettingsForm
            markDirty={props.markDirty}
            postSubmit={props.postSubmit}
            providers={providers.data!}
            config={protoConfig()}
          />
        </Match>
      </Switch>
    </Suspense>
  );
}

function InfoTooltip(props: {
  label: string;
  children: string;
  class?: string;
}) {
  return (
    <Tooltip>
      <TooltipTrigger class={props.class}>
        <div class="flex h-[40px] w-full items-center text-left">
          <L>{props.label}</L>

          <TbInfoCircle class="mx-1" />
        </div>
      </TooltipTrigger>

      <TooltipContent class="shrink">
        <div class="max-w-[50dvw] text-wrap">{props.children}</div>
      </TooltipContent>
    </Tooltip>
  );
}

function L(props: { children: JSX.Element }) {
  return (
    <div class="w-40">
      <Label>{props.children}</Label>
    </div>
  );
}
