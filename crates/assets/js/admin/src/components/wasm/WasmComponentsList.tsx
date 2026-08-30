import { createMemo, createSignal, For, Match, Show, Switch } from "solid-js";
import { A } from "@solidjs/router";
import {
  TbOutlinePuzzle,
  TbOutlineRefresh,
  TbOutlineArrowRight,
} from "solid-icons/tb";
import { Button } from "@/components/ui/button";
import { Callout, CalloutContent, CalloutTitle } from "@/components/ui/callout";
import { Badge } from "@/components/ui/badge";
import { Header } from "@/components/Header";
import { Spinner } from "@/components/Spinner";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  installWasmComponent,
  uninstallWasmComponent,
} from "@/lib/api/wasm-components";
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
  const image = () => {
    const icon = value();
    if (icon?.startsWith("<svg")) {
      return `data:image/svg+xml,${encodeURIComponent(icon)}`;
    }
    return icon?.match(/^data:image\//i) ? icon : undefined;
  };
  return (
    <Show when={image()} fallback={<TbOutlinePuzzle size={24} />}>
      <img src={image()} alt="" class="size-6" />
    </Show>
  );
}
export function WasmComponentsList(props: {
  components: WasmComponent[];
  isLoading: boolean;
  isError: boolean;
  refetch: () => void | Promise<unknown>;
}) {
  const components = createMemo(() => sortWasmComponents(props.components));
  const [dialog, setDialog] = createSignal<{
    component: WasmComponent;
    action: "install" | "remove";
  }>();
  const [pending, setPending] = createSignal(false);
  const [error, setError] = createSignal<"mutation" | "refresh">();

  const retryRefresh = async () => {
    if (pending()) return;
    setPending(true);
    setError(undefined);
    try {
      await props.refetch();
      setDialog(undefined);
    } catch {
      setError("refresh");
    } finally {
      setPending(false);
    }
  };

  const runAction = async () => {
    const current = dialog();
    if (!current || pending()) return;
    if (error() === "refresh") {
      await retryRefresh();
      return;
    }
    setPending(true);
    setError(undefined);
    try {
      const request = current.component.repo_id
        ? { RepoId: current.component.repo_id }
        : { Path: current.component.path };
      if (current.action === "install") await installWasmComponent(request);
      else await uninstallWasmComponent(request);
    } catch {
      setError("mutation");
      setPending(false);
      return;
    }

    try {
      await props.refetch();
      setDialog(undefined);
    } catch {
      setError("refresh");
    } finally {
      setPending(false);
    }
  };
  const running = createMemo(
    () => components().filter((c) => c.loaded && c.installed).length,
  );
  const hasPendingChanges = createMemo(() =>
    components().some((c) => c.loaded !== c.installed),
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
            disabled={props.isLoading}
            title="Refresh WASM components"
            aria-label="Refresh WASM components"
          >
            <TbOutlineRefresh /> Refresh
          </Button>
        }
      />
      <div class="min-w-0 overflow-x-auto p-4">
        <Callout
          variant={hasPendingChanges() ? "warning" : "default"}
          class="mb-3 text-sm"
        >
          <Show when={hasPendingChanges()}>
            <CalloutTitle>Restart required</CalloutTitle>
          </Show>
          <CalloutContent>
            Installing or removing a WASM component requires restarting the
            server. You can also run this while the server is off:
            <pre class="my-2 ml-4 whitespace-pre-wrap">
              trail [--depot=..] components add trailbase/auth_ui
            </pre>
          </CalloutContent>
        </Callout>
        <Switch>
          <Match when={props.isLoading}>
            <div class="flex h-64 items-center justify-center">
              <Spinner size={32} class="text-muted-foreground" />
            </div>
          </Match>
          <Match when={props.isError}>
            <Callout variant="error">
              <CalloutTitle>Unable to load WASM components</CalloutTitle>
              <CalloutContent>
                WASM components could not be loaded. Please try again.
                <div class="mt-2">
                  <Button onClick={() => props.refetch()}>Retry</Button>
                </div>
              </CalloutContent>
            </Callout>
          </Match>
          <Match when={!components().length}>
            <div class="py-12 text-center">
              <p>No WASM components installed.</p>
              <code>trail components add trailbase/auth_ui</code>
            </div>
          </Match>
          <Match when={true}>
            <div class="overflow-x-auto rounded-md border">
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
                                <div class="text-muted-foreground text-xs">
                                  Internal name: {c.name}
                                </div>
                                <Show when={c.description}>
                                  <div class="text-muted-foreground text-xs">
                                    {c.description}
                                  </div>
                                </Show>
                              </div>
                            </div>
                          </TableCell>
                          <TableCell>
                            <Badge variant={status.variant}>
                              {status.label}
                            </Badge>
                          </TableCell>
                          <TableCell>
                            {c.guest_runtime ?? "—"} / {c.version ?? "—"}
                          </TableCell>
                          <TableCell>
                            <code class="select-text">
                              {wasmComponentSource(c)}
                            </code>
                          </TableCell>
                          <TableCell>
                            <div class="flex flex-wrap gap-2">
                              <Show when={c.admin_ui_path}>
                                <A
                                  href={`/wasm/${c.name}`}
                                  class="inline-flex items-center gap-1 underline"
                                >
                                  Open dashboard <TbOutlineArrowRight />
                                </A>
                              </Show>
                              <Show when={c.installed}>
                                <Button
                                  type="button"
                                  variant="destructive"
                                  size="sm"
                                  title={`Remove ${c.name}`}
                                  aria-label={`Remove ${c.name}`}
                                  disabled={pending()}
                                  onClick={() => {
                                    setError(undefined);
                                    setDialog({
                                      component: c,
                                      action: "remove",
                                    });
                                  }}
                                >
                                  Remove
                                </Button>
                              </Show>
                              <Show
                                when={!c.loaded && !c.installed && !!c.repo_id}
                              >
                                <Button
                                  type="button"
                                  size="sm"
                                  title={`Install ${c.name}`}
                                  aria-label={`Install ${c.name}`}
                                  disabled={pending()}
                                  onClick={() => {
                                    setError(undefined);
                                    setDialog({
                                      component: c,
                                      action: "install",
                                    });
                                  }}
                                >
                                  Install
                                </Button>
                              </Show>
                            </div>
                          </TableCell>
                        </TableRow>
                      );
                    }}
                  </For>
                </TableBody>
              </Table>
            </div>
          </Match>
        </Switch>
      </div>
      <Dialog
        open={dialog() !== undefined}
        onOpenChange={(open) => {
          if (!pending() && !open) setDialog(undefined);
        }}
      >
        <DialogContent>
          <DialogTitle>
            {dialog()?.action === "install" ? "Install" : "Remove"}{" "}
            {dialog()?.component.name}
          </DialogTitle>
          <DialogDescription>
            {dialog()?.action === "install"
              ? `Install ${dialog()?.component.name}? Installed dashboards are trusted extensions and receive admin-context credentials. Only install components you trust. This change requires a server restart.`
              : `Remove ${dialog()?.component.name}? This change requires a server restart. The loaded instance continues until restart.`}
          </DialogDescription>
          <Show when={error() === "mutation"}>
            <Callout variant="error">
              <CalloutTitle>
                Unable to{" "}
                {dialog()?.action === "install" ? "install" : "remove"}{" "}
                component
              </CalloutTitle>
              <CalloutContent>
                The component could not be{" "}
                {dialog()?.action === "install" ? "installed" : "removed"}. No
                changes were made. Check the server and try again.
              </CalloutContent>
            </Callout>
          </Show>
          <Show when={error() === "refresh"}>
            <Callout variant="error">
              <CalloutTitle>Component changed, refresh failed</CalloutTitle>
              <CalloutContent>
                The component was changed, but the list could not be refreshed.
                Retry refresh before trying this action again.
              </CalloutContent>
            </Callout>
          </Show>
          <DialogFooter class="gap-2">
            <Button
              type="button"
              variant="outline"
              disabled={pending()}
              onClick={() => setDialog(undefined)}
            >
              Cancel
            </Button>
            <Button
              type="button"
              variant={
                dialog()?.action === "remove" ? "destructive" : "default"
              }
              disabled={pending()}
              onClick={runAction}
            >
              {pending()
                ? "Working…"
                : error() === "refresh"
                  ? "Retry refresh"
                  : dialog()?.action === "install"
                    ? "Install"
                    : "Remove"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
