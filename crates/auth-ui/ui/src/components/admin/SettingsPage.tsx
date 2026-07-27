import { createSignal } from "solid-js";
import { render } from "solid-js/web";
import type { Client } from "trailbase";

import { ScreenDimensions } from "@/components/admin/ScreenDimensions";

import {
  TextField,
  TextFieldLabel,
  TextFieldInput,
} from "@/components/ui/text-field";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { ErrorBoundary } from "@/components/ErrorBoundary";

import { adminClient } from "@/lib/admin-client";
import type { AuthUiSettings } from "@auth-ui/AuthUiSettings";

export function renderAdminUI(id: string) {
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
  const [settings, setSettings] = createSignal<AuthUiSettings | undefined>();

  let titleRef: HTMLInputElement | undefined;
  let iconRef: HTMLInputElement | undefined;

  return (
    <div class="flex flex-col gap-4 p-4">
      <Card>
        <CardHeader>
          <h2>Info</h2>
        </CardHeader>

        <CardContent>
          <p>
            update or reinstall with{" "}
            <span class="font-mono">
              {" trail components add trailbase/auth_ui "}
            </span>
          </p>
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
              <TextFieldInput ref={titleRef} type="text" />
            </div>
          </TextField>

          <TextField>
            <div class="flex items-center gap-2">
              <TextFieldLabel>Icon</TextFieldLabel>
              <TextFieldInput ref={iconRef} type="text" />
            </div>
          </TextField>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <h2>Debug</h2>
        </CardHeader>

        <CardContent class="flex flex-col gap-2">
          <ScreenDimensions />

          <span>Current Settings: {JSON.stringify(settings())}</span>

          <div class="flex gap-2">
            <Button
              onClick={() => {
                const client = buildClient();

                (async () => {
                  const settings = await fetchAuthUiSettings(client);
                  console.debug("Got", settings);
                  setSettings(settings);
                })();
              }}
            >
              Load Settings
            </Button>

            <Button
              onClick={() => {
                const client = buildClient();

                (async () => {
                  const updatedSettings = await updateAuthUiSettings(client, {
                    title: getOptionalValue(titleRef),
                    icon_url: getOptionalValue(iconRef),
                  });
                  setSettings(updatedSettings);
                })();
              }}
            >
              Save Settings
            </Button>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

function getOptionalValue(
  ref: HTMLInputElement | undefined,
): string | undefined {
  const value = ref?.value;
  if (value !== undefined || value !== "") {
    return value;
  }
  return undefined;
}

function buildClient(): Client {
  const client = adminClient();
  if (!client) {
    throw new Error("Tokens not found. Not logged in?");
  }
  if (client.tokens() === undefined) {
    throw new Error("Invalid tokens. Not logged in.");
  }

  return client;
}

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
