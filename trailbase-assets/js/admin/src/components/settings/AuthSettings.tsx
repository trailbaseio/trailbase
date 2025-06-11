import {
  createSignal,
  createMemo,
  For,
  Suspense,
  Switch,
  Match,
} from "solid-js";
import { useQuery } from "@tanstack/solid-query";
import { createForm } from "@tanstack/solid-form";

import {
  buildOptionalNumberFormField,
  buildOptionalBoolFormField,
  buildSecretFormField,
  buildOptionalTextFormField,
  type FormStateT,
} from "@/components/FormFields";
import type { FormApiT } from "@/components/FormFields";
import {
  TbCircleCheck,
  TbCircle,
  TbCirclePlus,
  TbArrowBackUp,
  TbTrash,
} from "solid-icons/tb";
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from "@/components/ui/accordion";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";

import {
  AuthConfig,
  Config,
  OAuthProviderConfig,
  OAuthProviderId,
} from "@proto/config";
import { createConfigQuery, setConfig } from "@/lib/config";
import { adminFetch } from "@/lib/fetch";
import { showSaveFileDialog } from "@/lib/utils";

import type { OAuthProviderResponse } from "@bindings/OAuthProviderResponse";
import type { OAuthProviderEntry } from "@bindings/OAuthProviderEntry";

// OAuth2 provider assets.
import openIdConnect from "@shared/assets/oauth2/oidc.svg";
import discord from "@shared/assets/oauth2/discord.svg";
import facebook from "@shared/assets/oauth2/facebook.svg";
import gitlab from "@shared/assets/oauth2/gitlab.svg";
import google from "@shared/assets/oauth2/google.svg";
import microsoft from "@shared/assets/oauth2/microsoft.svg";
import { useQueryClient } from "@tanstack/solid-query";

const assets = new Map<OAuthProviderId, string>([
  [OAuthProviderId.OIDC0, openIdConnect],
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
  namedOauthProviders: NamedOAuthProvider[];
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
    namedOauthProviders: providers.map((p): NamedOAuthProvider => {
      const config = idToConfig.get(p.id);
      const clientId = config?.clientId;

      return {
        provider: p,
        state: clientId
          ? {
              clientId: clientId,
              // NOTE: This is basically undefined since the config doesn't contain the striped secret.
              clientSecret: config?.clientSecret,
            }
          : undefined,
      };
    }),
  };
}

function proxyToConfig(proxy: AuthConfigProxy): AuthConfig {
  const config = AuthConfig.fromPartial({
    ...(proxy as Omit<AuthConfigProxy, "namedOauthProviders">),
  });
  config.oauthProviders = {};

  for (const entry of proxy.namedOauthProviders) {
    const p = entry.provider;
    const clientId = entry.state?.clientId;
    const clientSecret = entry.state?.clientSecret;

    if (clientId && clientSecret) {
      config.oauthProviders[p.name] = {
        providerId: p.id,
        // displayName: p.display_name,
        clientId,
        clientSecret,
      };
    } else {
      console.debug("Skipping: ", entry);
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

function createSetOnce<T>(initial: T): [
  () => T,
  (v: T) => void,
  {
    reset: (v: T) => void;
  },
] {
  let called = false;
  const [v, setV] = createSignal<T>(initial);

  const setter = (v: T) => {
    if (!called) {
      called = true;
      setV(() => v);
    }
  };

  return [v, setter, { reset: setV }];
}

function ProviderSettingsSubForm(props: {
  form: FormApiT<AuthConfigProxy>;
  index: number;
  provider: OAuthProviderEntry;
}) {
  const [original, setOnce, { reset }] = createSetOnce<
    OAuthProviderConfig | undefined
  >(undefined);

  const current = createMemo(() =>
    props.form.useStore((state: FormStateT<AuthConfigProxy>) => {
      if (state.isSubmitted) {
        reset(state.values.namedOauthProviders[props.index].state);
      }

      const s = state.values.namedOauthProviders[props.index].state;
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

  const bullet = () => {
    if (dirty()) {
      return <TbCirclePlus color="orange" />;
    }

    if (current()?.clientId !== undefined) {
      return <TbCircleCheck color="green" />;
    }

    return <TbCircle color="grey" />;
  };

  return (
    <AccordionItem value={`item-${props.provider.id}`}>
      <AccordionTrigger>
        <div class="flex items-center gap-4">
          {bullet()}
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
          <props.form.Field
            name={`namedOauthProviders[${props.index}].state.clientId`}
            validators={{
              onChange: ({ value }: { value: string | undefined }) => {
                if (value === "") return "Must not be empty";
              },
            }}
          >
            {buildOptionalTextFormField({ label: () => "Client Id" })}
          </props.form.Field>

          <props.form.Field
            name={`namedOauthProviders[${props.index}].state.clientSecret`}
            validators={{
              onChange: ({ value }: { value: string | undefined }) => {
                if (value === "") return "Must not be empty";
              },
            }}
          >
            {buildSecretFormField({ label: () => "Client Secret" })}
          </props.form.Field>
        </div>

        <div class="mr-4 flex items-center justify-end gap-2">
          <Button
            variant={"outline"}
            disabled={!dirty()}
            onClick={() => {
              props.form.setFieldValue(
                `namedOauthProviders[${props.index}].state`,
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
                `namedOauthProviders[${props.index}].state`,
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
function AuthSettingsForm(props: {
  config: Config;
  providers: OAuthProviderResponse;
  markDirty: () => void;
  postSubmit: () => void;
}) {
  const values = createMemo(() => {
    return configToProxy(
      props.providers.providers,
      props.config.auth ?? AuthConfig.create(),
    );
  });

  const queryClient = useQueryClient();
  const form = createForm(() => ({
    defaultValues: values() as AuthConfigProxy,
    onSubmit: async ({ value }) => {
      const newConfig = Config.decode(Config.encode(props.config).finish());
      newConfig.auth = proxyToConfig(value);

      console.debug("Submitting provider config:", value);
      await setConfig(queryClient, newConfig);

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
                {buildOptionalNumberFormField({
                  integer: true,
                  label: () => <div class={labelWidth}>Auth TTL [sec]</div>,
                  info: (
                    <p>
                      AuthToken TTL. Older tokens are invalid. A new AuthToken
                      can be minted given a valid refresh Token.
                    </p>
                  ),
                })}
              </form.Field>

              <form.Field name="refreshTokenTtlSec">
                {buildOptionalNumberFormField({
                  integer: true,
                  label: () => <div class={labelWidth}>Refresh TTL [sec]</div>,
                  info: (
                    <p>
                      RefreshToken TTL. Older tokens are invalid. A refresh
                      token can only be renewed by users logging in anew.
                    </p>
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
                    <div class={labelWidth}>Disable Password Registration</div>
                  ),
                  info: (
                    <p>
                      When disabled new users can only sign up via OAuth.
                      Existing users will can continue to sign in using
                      password-based auth.
                    </p>
                  ),
                })}
              </form.Field>

              <form.Field name="passwordMinimalLength">
                {buildOptionalNumberFormField({
                  label: () => <div class={labelWidth}>Min Length</div>,
                  info: (
                    <p>
                      Minimal length for new passwords [Default 8]. Does not
                      affect existing registrations unless users choose to
                      change their password.
                    </p>
                  ),
                })}
              </form.Field>

              <form.Field name="passwordMustContainUpperAndLowerCase">
                {buildOptionalBoolFormField({
                  label: () => (
                    <div class={labelWidth}>
                      Must Contain Upper {"&"} Lower Case
                    </div>
                  ),
                  info: (
                    <p>
                      Passwords must contain both, upper and lower case
                      characters. Does not affect existing registrations unless
                      users choose to change their password.
                    </p>
                  ),
                })}
              </form.Field>

              <form.Field name="passwordMustContainDigits">
                {buildOptionalBoolFormField({
                  label: () => (
                    <div class={labelWidth}>Must Contain Digits</div>
                  ),
                  info: (
                    <p>
                      Passwords must additionally contain digits. Does not
                      affect existing registrations unless users choose to
                      change their password.
                    </p>
                  ),
                })}
              </form.Field>

              <form.Field name="passwordMustContainSpecialCharacters">
                {buildOptionalBoolFormField({
                  label: () => (
                    <div class={labelWidth}>Must Contain Special</div>
                  ),
                  info: (
                    <p>
                      Passwords must additionally contain special, i.e.,
                      non-alphanumeric characters.. Does not affect existing
                      registrations unless users choose to change their
                      password.
                    </p>
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
            <form.Field name="namedOauthProviders">
              {(_field) => {
                return (
                  <Accordion multiple={false} collapsible class="w-full">
                    <For each={values().namedOauthProviders}>
                      {(provider, index) => {
                        // Skip OIDC provider for now until we expand the form to render the extra fields.
                        if (provider.provider.id === OAuthProviderId.OIDC0) {
                          return null;
                        }

                        return (
                          <ProviderSettingsSubForm
                            form={form}
                            index={index()}
                            provider={provider.provider}
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
              <Button
                variant="default"
                onClick={() =>
                  (async () => {
                    // NOTE: we cannot just have a <a download /> here since admin APIs require CSRF token.
                    const response = await adminFetch(`/public_key`);
                    const keyText = await response.text();

                    await showSaveFileDialog({
                      contents: keyText,
                      filename: "pubkey.pep",
                    });
                  })().catch(console.error)
                }
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

const labelWidth = "w-40";
