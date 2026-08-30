import { createMemo, Match, Switch } from "solid-js";
import { useParams } from "@solidjs/router";
import { useQuery } from "@tanstack/solid-query";
import { listWasmComponents } from "@/lib/api/wasm-components";
import { Spinner } from "@/components/Spinner";
import { WasmComponentDetails } from "@/components/wasm/WasmComponentDetails";
import { WasmComponentsList } from "@/components/wasm/WasmComponentsList";

export function WasmPage() {
  const params = useParams<{ name?: string }>();
  const query = useQuery(() => ({
    queryKey: ["wasm-components"],
    queryFn: listWasmComponents,
  }));
  const components = createMemo(() => query.data?.components ?? []);
  const findComponent = createMemo(() =>
    components().find((c) => c.name === params.name),
  );
  return (
    <Switch>
      <Match when={query.isLoading}>
        <div class="flex h-64 items-center justify-center">
          <Spinner size={32} class="text-muted-foreground" />
        </div>
      </Match>
      <Match when={params.name !== undefined && findComponent() !== undefined}>
        <WasmComponentDetails component={findComponent()!} sandboxed={true} />
      </Match>
      <Match
        when={params.name !== undefined && !findComponent() && !query.isError}
      >
        A component with name "{params.name}" is not installed.
      </Match>
      <Match when={true}>
        <WasmComponentsList
          components={components()}
          isLoading={query.isLoading}
          isError={query.isError}
          refetch={query.refetch}
        />
      </Match>
    </Switch>
  );
}
