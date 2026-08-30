import { createMemo, createSignal, For, Match, Show, Switch } from "solid-js";
import { template } from "solid-js/web";
import { useQuery } from "@tanstack/solid-query";
import { A } from "@solidjs/router";
import {
  TbOutlinePuzzle,
  TbOutlineArrowRight,
  TbOutlineDownload,
  TbOutlineTrash,
} from "solid-icons/tb";

import { Button } from "@/components/ui/button";
import { Callout } from "@/components/ui/callout";
import {
  Card,
  CardContent,
  CardTitle,
  CardDescription,
} from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogTrigger,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";
import { Header } from "@/components/Header";
import { Spinner } from "@/components/Spinner";

import {
  listWasmComponents,
  installWasmComponent,
  uninstallWasmComponent,
} from "@/lib/api/wasm-components";
import type { WasmComponent } from "@bindings/WasmComponent";
import { cn } from "@/lib/utils";

export type WasmComponentStatus = {
  key: "running" | "available" | "install-pending" | "removal-pending";
  label: string;
  priority: number;
  variant: "success" | "secondary" | "warning";
};

export function wasmComponentStatus(
  component: WasmComponent,
): WasmComponentStatus {
  if (component.loaded && component.installed) {
    return { key: "running", label: "Running", priority: 1, variant: "success" };
  }
  if (!component.loaded && component.installed) {
    return {
      key: "install-pending",
      label: "Install pending restart",
      priority: 0,
      variant: "warning",
    };
  }
  if (component.loaded && !component.installed) {
    return {
      key: "removal-pending",
      label: "Removal pending restart",
      priority: 0,
      variant: "warning",
    };
  }
  return {
    key: "available",
    label: "Available",
    priority: 2,
    variant: "secondary",
  };
}

export function sortWasmComponents(components: WasmComponent[]): WasmComponent[] {
  return [...components].sort((a, b) => {
    const priority =
      wasmComponentStatus(a).priority - wasmComponentStatus(b).priority;
    return priority || (a.display_name ?? a.name).localeCompare(b.display_name ?? b.name);
  });
}

export function wasmComponentSource(component: WasmComponent): string {
  return component.repo_id ?? component.path;
}

function ComponentIcon(props: { icon?: string }) {
  const icon = createMemo(() => props.icon?.trim());

  // Inline SVGs avoid <img> to keep 'em work with background colors.
  const buildSvg = (icon: string) => {
    return template(icon.trim())();
  };

  return (
    <Switch>
      <Match when={icon()?.startsWith("<svg") ? icon() : undefined}>
        {(icon) => <div class="size-6 [&>svg]:size-6">{buildSvg(icon())}</div>}
      </Match>

      <Match when={icon()?.startsWith("data:") ? icon() : undefined}>
        {(icon) => <img src={icon()} class="size-6" />}
      </Match>

      <Match when={true}>
        <TbOutlinePuzzle size={24} />
      </Match>
    </Switch>
  );
}

function ComponentCardContent(props: {
  component: WasmComponent;
  refetch: () => void;
  hasDetails: boolean;
}) {
  const displayName = () =>
    props.component.display_name ?? props.component.name;

  const skew = () => props.component.loaded != props.component.installed;

  return (
    <CardContent class={cn("flex p-4", skew() && "bg-error")}>
      <div class="text-muted-foreground size-10 shrink-0 content-center">
        <ComponentIcon icon={props.component.icon ?? undefined} />
      </div>

      <div class="flex w-full gap-2">
        <div class="flex grow flex-col justify-start">
          <div class="flex h-full flex-wrap items-center gap-2">
            <CardTitle class={props.hasDetails ? "" : "text-muted-foreground"}>
              {displayName()}
            </CardTitle>

            <Show when={displayName() !== props.component.name}>
              <span class="text-muted-foreground text-xs">
                {props.component.name}
              </span>
            </Show>

            <Show when={props.component.version}>
              <span class="text-muted-foreground text-xs">
                {`@${props.component.version}`}
              </span>
            </Show>
          </div>

          <Show when={props.component.description}>
            <CardDescription>{props.component.description}</CardDescription>
          </Show>
        </div>

        <Show
          when={props.component.repo_id && props.component.installed === false}
        >
          <InstallButton {...props} />
        </Show>

        <Show when={props.component.installed === true}>
          <UninstallButton {...props} />
        </Show>

        <Show when={props.component.admin_ui_path}>
          <div class="text-muted-foreground hover:bg-accent hover:text-accent-foreground content-center rounded-sm p-2">
            <TbOutlineArrowRight size={18} />
          </div>
        </Show>
      </div>
    </CardContent>
  );
}

function ComponentCard(props: {
  component: WasmComponent;
  refetch: () => void;
}) {
  const hasDetails = () => !!props.component.admin_ui_path;

  return (
    <Card>
      <Switch>
        <Match when={hasDetails()}>
          <A href={`/wasm/${props.component.name}`}>
            <ComponentCardContent {...props} hasDetails={true} />
          </A>
        </Match>

        <Match when={true}>
          <ComponentCardContent {...props} hasDetails={false} />
        </Match>
      </Switch>
    </Card>
  );
}

export function WasmComponentsList() {
  const wasmComponents = useQuery(() => ({
    queryKey: ["wasm-components"],
    queryFn: listWasmComponents,
  }));

  const components = (): WasmComponent[] => {
    const components = [...(wasmComponents.data?.components ?? [])];
    if (import.meta.env.DEV) {
      components.push({
        name: "[DEV]injected_debug_default",
        path: "wasm/fake_component.wasm",
        loaded: false,
        installed: false,
      });
    }
    return components;
  };

  return (
    <div>
      <Header title="WASM Components" />

      <div class="flex flex-col gap-3 p-4">
        <Callout class="text-sm">
          Installing or removing a WASM component currently requires the server
          to be restarted in order to take effect. Alternatively, to avoid skew
          between a component being installed on the file system and loaded for
          use, you can run the CLI while the server is off, e.g.:
          <pre class="my-2 ml-4 whitespace-pre-wrap">
            trail [--depot=..] components add trailbase/auth_ui
          </pre>
        </Callout>

        <Switch>
          <Match when={wasmComponents.isLoading}>
            <div class="flex h-64 items-center justify-center">
              <Spinner size={32} class="text-muted-foreground" />
            </div>
          </Match>

          <Match when={wasmComponents.isError}>
            {`${wasmComponents.error}`}
          </Match>

          <Match when={wasmComponents.isSuccess}>
            <For each={components()}>
              {(c) => (
                <ComponentCard component={c} refetch={wasmComponents.refetch} />
              )}
            </For>
          </Match>
        </Switch>
      </div>
    </div>
  );
}

function InstallButton(props: {
  component: WasmComponent;
  refetch: () => void;
}) {
  const [open, setOpen] = createSignal(false);

  return (
    <Dialog open={open()} onOpenChange={setOpen}>
      <DialogTrigger>
        <Button variant="ghost" size="icon">
          <TbOutlineDownload />
        </Button>
      </DialogTrigger>

      <DialogContent>
        <DialogTitle>Confirmation</DialogTitle>

        <p>
          For the installing of the WASM component to take effect, the server
          needs to be restarted.
        </p>

        <DialogFooter class="gap-2">
          <Button variant="outline" onClick={() => setOpen(false)}>
            Back
          </Button>

          <Button
            variant="destructive"
            onClick={() => {
              // Install the component.
              const repoId = props.component.repo_id;
              if (!repoId) {
                throw new Error("missing repo id");
              }

              (async () => {
                await installWasmComponent({
                  RepoId: repoId,
                });

                props.refetch();
                setOpen(false);
              })();
            }}
          >
            Proceed
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function UninstallButton(props: {
  component: WasmComponent;
  refetch: () => void;
}) {
  const [open, setOpen] = createSignal(false);

  return (
    <Dialog open={open()} onOpenChange={setOpen}>
      <DialogTrigger>
        <Button
          variant="ghost"
          size="icon"
          onClick={(e) => {
            // Do not follow the link to the dashboard.
            e.preventDefault();
          }}
        >
          <TbOutlineTrash />
        </Button>
      </DialogTrigger>

      <DialogContent>
        <DialogTitle>Confirmation</DialogTitle>

        <p>
          For the removal of the WASM component to take effect, the server needs
          to be restarted.
        </p>

        <DialogFooter class="gap-2">
          <Button variant="outline" onClick={() => setOpen(false)}>
            Back
          </Button>

          <Button
            variant="destructive"
            onClick={() => {
              // Delete the component.
              (async () => {
                uninstallWasmComponent(
                  props.component.repo_id
                    ? {
                        RepoId: props.component.repo_id,
                      }
                    : {
                        Path: props.component.path,
                      },
                );

                props.refetch();
                setOpen(false);
              })();
            }}
          >
            Proceed
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
