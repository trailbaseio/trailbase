import { createResource, Show } from "solid-js";
import { render } from "solid-js/web";
import { TbOutlineRefresh } from "solid-icons/tb";
import { useStore } from "@nanostores/solid";
import type { Client } from "trailbase";

import { ScreenDimensions } from "@/components/admin/ScreenDimensions";

import {
  TextField,
  TextFieldLabel,
  TextFieldInput,
} from "@/components/ui/text-field";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardFooter,
  CardHeader,
} from "@/components/ui/card";
import { ErrorBoundary } from "@/components/ErrorBoundary";

import { $client, installPostMessageHandler } from "@/lib/admin-client";
import type { AuthUiSettings } from "@auth-ui/AuthUiSettings";

export function renderAdminUI(id: string) {
  // Install handler to receive tokens from shell via postMessage.
  installPostMessageHandler();

  render(
    () => (
      <ErrorBoundary>
        <SettingsPage />
      </ErrorBoundary>
    ),
    document.getElementById(id)!,
  );
}

function SettingsPage() {
  const client = useStore($client);
  const [settings, { mutate: setSettings, refetch }] = createResource(
    client,
    async (c) => {
      if (!c) {
        console.debug("undefined client");
        return {} as AuthUiSettings;
      }
      // const client = buildClient();
      return await fetchAuthUiSettings(c);
    },
  );

  let titleRef: HTMLInputElement | undefined;
  let iconRef: HTMLInputElement | undefined;

  return (
    <div class="flex flex-col gap-4 p-4">
      <Card>
        <CardHeader>
          <h2>Info</h2>
        </CardHeader>

        <CardContent>
          <p class="text-sm">
            To update or re-install the first-party auth-ui component, simply
            run the following:
          </p>

          <pre class="mx-4 my-2 whitespace-pre-wrap">
            trail components add trailbase/auth_ui
          </pre>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <h2>Settings</h2>
        </CardHeader>

        <CardContent class="flex flex-col gap-2">
          <TextField>
            <div class="flex items-center gap-2">
              <TextFieldLabel>Title</TextFieldLabel>
              <TextFieldInput
                ref={titleRef}
                type="text"
                value={settings()?.title ?? ""}
                placeholder="title"
              />
            </div>
          </TextField>

          <TextField>
            <div class="flex items-center gap-2">
              <TextFieldLabel>Icon</TextFieldLabel>
              <TextFieldInput
                ref={iconRef}
                type="text"
                value={settings()?.icon_url ?? ""}
                placeholder="url, e.g. https://logo.org/logo.png"
              />
            </div>
          </TextField>
        </CardContent>

        <CardFooter class="flex justify-end gap-2">
          <Button size="icon" variant="outline" onClick={() => refetch()}>
            <TbOutlineRefresh />
          </Button>

          <Button
            onClick={() => {
              const c = client();
              if (!c) {
                return;
              }

              (async () => {
                const settings: AuthUiSettings = {
                  title: getOptionalValue(titleRef),
                  icon_url: getOptionalValue(iconRef),
                };
                const updatedSettings = await updateAuthUiSettings(c, settings);
                setSettings(updatedSettings);
              })();
            }}
          >
            Submit
          </Button>
        </CardFooter>
      </Card>

      <Show when={import.meta.env.DEV}>
        <Card>
          <CardHeader>
            <h2>Debug</h2>
          </CardHeader>

          <CardContent class="flex flex-col gap-2">
            <ScreenDimensions />

            <span>Current Settings: {JSON.stringify(settings())}</span>
          </CardContent>
        </Card>
      </Show>
    </div>
  );
}

function getOptionalValue(
  ref: HTMLInputElement | undefined,
): string | undefined {
  const value = ref?.value?.trim();
  if (value) {
    return value;
  }
  return undefined;
}

// function buildClient(): Client {
//   const client = adminClient();
//   if (!client) {
//     throw new Error("Tokens not found. Not logged in?");
//   }
//   if (client.tokens() === undefined) {
//     throw new Error("Invalid tokens. Not logged in.");
//   }
//
//   return client;
// }

async function fetchAuthUiSettings(client: Client) {
  const response = await client.fetch("_/auth/admin/settings/", {
    throwOnError: true,
  });
  return (await response.json()) as AuthUiSettings;
}

async function updateAuthUiSettings(
  client: Client,
  settings: AuthUiSettings,
): Promise<AuthUiSettings> {
  await client.fetch("_/auth/admin/settings/", {
    method: "POST",
    body: JSON.stringify(settings),
    throwOnError: true,
  });

  return settings;
}
