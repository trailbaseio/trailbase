import { createMemo, Match, Switch } from "solid-js";
import { A, useParams } from "@solidjs/router";
import { useQuery } from "@tanstack/solid-query";
import { listWasmComponents } from "@/lib/api/wasm-components";
import { WasmComponentDetails } from "@/components/wasm/WasmComponentDetails";
import { WasmComponentsList } from "@/components/wasm/WasmComponentsList";

export function WasmPage() {
  const params = useParams<{ name?: string }>();
  const query = useQuery(() => ({
    queryKey: ["wasm-components"],
    queryFn: listWasmComponents,
  }));
  const components = createMemo(() => query.data?.components ?? []);
  const refetch = async (): Promise<void> => {
    await query.refetch({ throwOnError: true });
  };
  const findComponent = createMemo(() =>
    components().find((c) => c.name === params.name),
  );
  return (
    <Switch>
      <Match
        when={
          params.name !== undefined &&
          !query.isLoading &&
          !query.isError &&
          findComponent() !== undefined
        }
      >
        <WasmComponentDetails component={findComponent()!} sandboxed={true} />
      </Match>
      <Match
        when={
          params.name !== undefined &&
          !findComponent() &&
          !query.isLoading &&
          !query.isError
        }
      >
        <div class="flex size-full flex-col items-center justify-center gap-3 p-6 text-center">
          <h2 class="text-lg font-semibold">Component not installed</h2>
          <p class="text-muted-foreground m-0">
            No WASM component named <code>{params.name}</code> is installed.
          </p>
          <A href="/wasm">Back to WASM components</A>
        </div>
      </Match>
      <Match when={true}>
        <WasmComponentsList
          components={components()}
          isLoading={query.isLoading}
          isError={query.isError}
          refetch={refetch}
        />
      </Match>
    </Switch>
  );
}
