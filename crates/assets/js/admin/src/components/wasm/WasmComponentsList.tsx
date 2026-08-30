import { createMemo, For, Match, Show, Switch } from "solid-js";
import { template } from "solid-js/web";
import { A } from "@solidjs/router";
import {
  TbOutlinePuzzle,
  TbOutlineRefresh,
  TbOutlineArrowRight,
} from "solid-icons/tb";
import { Button } from "@/components/ui/button";
import { Callout } from "@/components/ui/callout";
import { Header } from "@/components/Header";
import { Spinner } from "@/components/Spinner";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import type { WasmComponent } from "@bindings/WasmComponent";

export type WasmComponentStatus = {
  key: "running" | "available" | "install-pending" | "removal-pending";
  label: string;
  priority: number;
  variant: "success" | "secondary" | "warning";
};
export function wasmComponentStatus(c: WasmComponent): WasmComponentStatus {
  if (c.loaded && c.installed)
    return {
      key: "running",
      label: "Running",
      priority: 1,
      variant: "success",
    };
  if (!c.loaded && c.installed)
    return {
      key: "install-pending",
      label: "Install pending restart",
      priority: 0,
      variant: "warning",
    };
  if (c.loaded && !c.installed)
    return {
      key: "removal-pending",
      label: "Removal pending restart",
      priority: 0,
      variant: "warning",
    };
  return {
    key: "available",
    label: "Available",
    priority: 2,
    variant: "secondary",
  };
}
export function sortWasmComponents(cs: WasmComponent[]) {
  return [...cs].sort(
    (a, b) =>
      wasmComponentStatus(a).priority - wasmComponentStatus(b).priority ||
      (a.display_name ?? a.name).localeCompare(b.display_name ?? b.name),
  );
}
export function wasmComponentSource(c: WasmComponent) {
  return c.repo_id ?? c.path;
}
function Icon(props: { value?: string }) {
  const value = () => props.value?.trim();
  return (
    <Switch>
      <Match when={value()?.startsWith("<svg")}>
        {(v) => <div class="size-6 [&>svg]:size-6">{template(v()!)()}</div>}
      </Match>
      <Match when={value()?.startsWith("data:")}>
        {(v) => <img src={v()!} alt="" class="size-6" />}
      </Match>
      <Match when={true}>
        <TbOutlinePuzzle size={24} />
      </Match>
    </Switch>
  );
}
export function WasmComponentsList(props: {
  components: WasmComponent[];
  isLoading: boolean;
  isError: boolean;
  refetch: () => void;
}) {
  const components = createMemo(() => sortWasmComponents(props.components));
  const running = createMemo(
    () => components().filter((c) => c.loaded && c.installed).length,
  );
  return (
    <div>
      <Header
        title="WASM Components"
        description={`${components().length} total · ${running()} running`}
        right={
          <Button
            variant="outline"
            onClick={() => props.refetch()}
            title="Refresh WASM components"
            aria-label="Refresh WASM components"
          >
            <TbOutlineRefresh /> Refresh
          </Button>
        }
      />
      <div class="min-w-0 overflow-x-auto p-4">
        <Callout title="Restart required" class="mb-3 text-sm">
          Installing or removing a WASM component requires restarting the
          server. You can also run this while the server is off:
          <pre class="my-2 ml-4 whitespace-pre-wrap">
            trail [--depot=..] components add trailbase/auth_ui
          </pre>
        </Callout>
        <Switch>
          <Match when={props.isLoading}>
            <div class="flex h-64 items-center justify-center">
              <Spinner size={32} class="text-muted-foreground" />
            </div>
          </Match>
          <Match when={props.isError}>
            <Callout variant="error" title="Unable to load WASM components">
              <Button onClick={() => props.refetch()}>Retry</Button>
            </Callout>
          </Match>
          <Match when={!components().length}>
            <div class="py-12 text-center">
              <p>No WASM components installed.</p>
              <code>trail components add trailbase/auth_ui</code>
            </div>
          </Match>
          <Match when={true}>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Component</TableHead>
                  <TableHead>State</TableHead>
                  <TableHead>Runtime / Version</TableHead>
                  <TableHead>Source</TableHead>
                  <TableHead>Actions</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                <For each={components()}>
                  {(c) => {
                    const status = wasmComponentStatus(c);
                    return (
                      <TableRow>
                        <TableCell>
                          <div class="flex items-start gap-2">
                            <Icon value={c.icon} />
                            <div>
                              <div class="font-medium">
                                {c.display_name ?? c.name}
                              </div>
                              <Show when={c.display_name}>
                                <div class="text-muted-foreground text-xs">
                                  {c.name}
                                </div>
                              </Show>
                              <Show when={c.description}>
                                <div class="text-muted-foreground text-xs">
                                  {c.description}
                                </div>
                              </Show>
                            </div>
                          </div>
                        </TableCell>
                        <TableCell>{status.label}</TableCell>
                        <TableCell>
                          {c.guest_runtime ?? "—"} / {c.version ?? "—"}
                        </TableCell>
                        <TableCell>
                          <code>{wasmComponentSource(c)}</code>
                        </TableCell>
                        <TableCell>
                          <Show when={c.admin_ui_path}>
                            <A
                              href={`/wasm/${c.name}`}
                              class="inline-flex items-center gap-1 underline"
                            >
                              Open dashboard <TbOutlineArrowRight />
                            </A>
                          </Show>
                        </TableCell>
                      </TableRow>
                    );
                  }}
                </For>
              </TableBody>
            </Table>
          </Match>
        </Switch>
      </div>
    </div>
  );
}
