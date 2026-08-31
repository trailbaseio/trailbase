import { QueryClient, useQuery } from "@tanstack/solid-query";

import { Config } from "@proto/config";
import { GetConfigResponse, UpdateConfigRequest } from "@proto/config_api";
import { adminFetch } from "@/lib/fetch";
import { showToast } from "@/components/ui/toast";

export function createConfigQuery() {
  async function getConfig(): Promise<GetConfigResponse> {
    const response = await adminFetch("/config");
    const array = new Uint8Array(await (await response.blob()).arrayBuffer());
    return GetConfigResponse.decode(array);
  }

  return useQuery(() => ({
    queryKey: _configKey,
    queryFn: async () => {
      return getConfig();
    },
    refetchInterval: 120 * 1000,
    refetchOnMount: false,
  }));
}

export async function setConfig(opts: {
  client: QueryClient;
  config: Config;
  throw: boolean;
}): Promise<void> {
  async function updateConfig(request: UpdateConfigRequest) {
    await adminFetch("/config", {
      method: "POST",
      headers: {
        "Content-Type": "application/octet-stream",
      },
      body: new Uint8Array(UpdateConfigRequest.encode(request).finish()),
      throwOnError: true,
    });
  }

  const hash = opts.client.getQueryData<GetConfigResponse>(_configKey)?.hash;
  if (!hash) {
    if (opts.throw) throw new Error("Missing config hash");
    showToast({
      title: "Config update failed",
      description: "Please refresh and try again.",
      variant: "error",
    });
    return;
  }

  const request: UpdateConfigRequest = {
    config: opts.config,
    hash,
  };

  if (opts.throw) {
    await updateConfig(request);
  } else {
    try {
      await updateConfig(request);
    } catch (err) {
      showToast({
        title: "Config update failed",
        description: "Please refresh and try again.",
        variant: "error",
      });
      return;
    }
  }

  // Trigger re-fetch after updating config.  await invalidateConfig(opts.client);
}

export function invalidateConfig(queryClient: QueryClient) {
  return queryClient.invalidateQueries({
    queryKey: _configKey,
  });
}

const _configKey = ["admin", "proto_config"];
