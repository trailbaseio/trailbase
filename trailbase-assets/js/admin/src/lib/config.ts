import { QueryClient, useQuery } from "@tanstack/solid-query";

import { Config } from "@proto/config";
import { GetConfigResponse, UpdateConfigRequest } from "@proto/config_api";
import { adminFetch } from "@/lib/fetch";

export async function setConfig(
  queryClient: QueryClient,
  config: Config,
): Promise<void> {
  const data = queryClient.getQueryData<GetConfigResponse>(key);
  const hash = data?.hash;
  if (!hash) {
    console.error("Missing hash from:", data);
    return;
  }

  const request: UpdateConfigRequest = {
    config,
    hash,
  };
  console.debug("Updating config:", request);
  await updateConfig(request);

  // Trigger refetch after updating config.
  invalidateConfig(queryClient);
}

export function invalidateAllAdminQueries(queryClient: QueryClient) {
  queryClient.invalidateQueries({
    queryKey: ["admin"],
  });
}

export function invalidateConfig(queryClient: QueryClient) {
  queryClient.invalidateQueries({
    queryKey: key,
  });
}

export function createConfigQuery() {
  return useQuery(() => ({
    queryKey: key,
    queryFn: async () => {
      const config = await getConfig();
      console.debug("Fetched config:", config);
      return config;
    },
    refetchInterval: 120 * 1000,
    refetchOnMount: false,
  }));
}

async function getConfig(): Promise<GetConfigResponse> {
  const response = await adminFetch("/config");
  const array = new Uint8Array(await (await response.blob()).arrayBuffer());
  return GetConfigResponse.decode(array);
}

async function updateConfig(request: UpdateConfigRequest): Promise<void> {
  await adminFetch("/config", {
    method: "POST",
    headers: {
      "Content-Type": "application/octet-stream",
    },
    body: UpdateConfigRequest.encode(request).finish(),
  });
}

const key = ["admin", "proto_config"];
