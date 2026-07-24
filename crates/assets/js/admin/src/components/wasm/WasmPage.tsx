import { createMemo, Match, Switch } from "solid-js";
import { useParams } from "@solidjs/router";
import { useQuery } from "@tanstack/solid-query";

import { listWasmComponents } from "@/lib/api/wasm-components";

import { Spinner } from "@/components/Spinner";
import { WasmComponentDetails } from "@/components/wasm/WasmComponentDetails";
import { WasmComponentsList } from "@/components/wasm/WasmComponentsList";
import { find } from "@antv/x6/lib/common/dom/elem";

export function WasmPage() {
  const params = useParams<{ name?: string }>();

  const wasmComponents = useQuery(() => ({
    queryKey: ["wasm-components"],
    queryFn: listWasmComponents,
  }));

  const findComponent = createMemo(() =>
    wasmComponents.data?.components.find((m) => m.name === params.name),
  );

  return (
    <Switch>
      <Match when={wasmComponents.isLoading}>
        <div class="flex h-64 items-center justify-center">
          <Spinner size={32} class="text-muted-foreground" />
        </div>
      </Match>

      <Match when={wasmComponents.isError}>{`${wasmComponents.error}`}</Match>

      <Match when={params.name !== undefined && findComponent() !== undefined}>
        <WasmComponentDetails component={findComponent()!} />
      </Match>

      <Match when={params.name !== undefined && findComponent() === undefined}>
        A component with name "{params.name}" is not installed.
      </Match>

      <Match when={true}>
        <WasmComponentsList />
      </Match>
    </Switch>
  );
}
