import { QueryClient, createQuery } from "@tanstack/solid-query";

import { Config } from "@proto/config";
import { GetConfigResponse, UpdateConfigRequest } from "@proto/config_api";
import { adminFetch } from "@/lib/fetch";

const defaultKey = ["default"];

function createClient(): QueryClient {
  return new QueryClient();
}
const queryClient = createClient();

export async function setConfig(config: Config) {
  const data = queryClient.getQueryData<GetConfigResponse>(defaultKey);
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
  const response = await updateConfig(request);

  queryClient.invalidateQueries();

  return response;
}

export function createConfigQuery() {
  return createQuery(
    () => ({
      queryKey: defaultKey,
      queryFn: async () => {
        const config = await getConfig();
        console.debug("Fetched config:", config);
        return config;
      },
      refetchInterval: 120 * 1000,
      refetchOnMount: false,
    }),
    () => queryClient,
  );
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
