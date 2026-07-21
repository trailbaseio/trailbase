import { Match, Switch } from "solid-js";
import { useParams } from "@solidjs/router";

import { WasmComponentDetails } from "@/components/wasm/WasmComponentDetails";
import { WasmComponentsList } from "@/components/wasm/WasmComponentsList";

export function WasmPage() {
  const params = useParams<{ name?: string }>();

  return (
    <Switch>
      <Match when={params.name !== undefined}>
        <WasmComponentDetails name={params.name!} />
      </Match>

      <Match when={true}>
        <WasmComponentsList />
      </Match>
    </Switch>
  );
}
