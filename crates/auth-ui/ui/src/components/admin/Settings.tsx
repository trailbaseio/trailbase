import { createSignal } from "solid-js";

import { Button } from "@/components/ui/button";
import { adminClient } from "@/lib/admin-client";

export function Settings() {
  const [settings, setSettings] = createSignal<string | undefined>();

  return (
    <div class="flex flex-col gap-2">
      <span>Settings: {settings()}</span>

      <div class="flex gap-2">
        <Button
          onClick={() => {
            const client = adminClient();
            if (!client) {
              console.warn("skip, no client. not logged in?");
              return;
            }
            if (client.tokens() === undefined) {
              console.warn("not logged in");
              return;
            }

            (async () => {
              const response = await client.fetch("_/auth/admin/settings/", {});
              const body = await response.text();
              console.debug("Got", body);
              setSettings(body);
            })();
          }}
        >
          Load Settings
        </Button>

        <Button
          onClick={() => {
            const client = adminClient();
            if (!client) {
              console.warn("skip, no client. not logged in?");
              return;
            }
            if (client.tokens() === undefined) {
              console.warn("not logged in");
              return;
            }

            (async () => {
              await client.fetch("_/auth/admin/settings/", {
                method: "POST",
                body: JSON.stringify({
                  test: "value",
                }),
              });
            })();
          }}
        >
          Save Settings
        </Button>
      </div>
    </div>
  );
}
