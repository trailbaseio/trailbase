import {
  createContext,
  createSignal,
  useContext,
  For,
  Match,
  Show,
  Switch,
} from "solid-js";
import type { Accessor } from "solid-js";
import { useNavigate, Location } from "@solidjs/router";
import {
  TbOutlineDatabase,
  TbOutlineEdit,
  TbOutlineUsers,
  TbOutlineChartDots3,
  TbOutlineTimeline,
  TbOutlineSettings,
  TbOutlineMoon,
  TbOutlineSun,
  TbOutlinePackage,
  TbOutlineApi,
} from "solid-icons/tb";

import { AuthButton } from "@/components/auth/AuthButton";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { Version } from "@/components/Version";

import { createSystemInfoQuery } from "@/lib/api/info";
import {
  createTheme,
  currentTheme,
  applyResolvedTheme,
  $themePreference,
} from "@/lib/theme";

import logo from "@/assets/logo_104.webp";

const BASE = import.meta.env.BASE_URL;
const groups = [
  [
    "Data",
    [
      [`${BASE}/table/`, TbOutlineDatabase, "Tables"],
      [`${BASE}/editor`, TbOutlineEdit, "SQL Editor"],
      [`${BASE}/erd`, TbOutlineChartDots3, "ERD"],
    ],
  ],
  [
    "Operate",
    [
      [`${BASE}/auth`, TbOutlineUsers, "Accounts"],
      [`${BASE}/wasm/`, TbOutlinePackage, "WASM"],
      [`${BASE}/logs`, TbOutlineTimeline, "Logs"],
      [`${BASE}/openapi`, TbOutlineApi, "OpenAPI"],
    ],
  ],
  ["System", [[`${BASE}/settings/`, TbOutlineSettings, "Settings"]]],
] as const;

type NavbarContextT = {
  dirty: Accessor<boolean>;
  setDirty: (dirty: boolean) => void;
};

export const NavbarContext = createContext<NavbarContextT | null>(null);

export function useNavbar(): NavbarContextT | undefined {
  const context = useContext(NavbarContext);
  if (context) {
    return context;
  }

  console.warn("useNavbar() called outside a NavbarContext");
}

type DirtyDialogState = {
  next: string;
};

function NavbarItems(props: { location: Location; horizontal: boolean }) {
  const navbar = useNavbar();
  const [dirtyDialog, setDirtyDialog] = createSignal<DirtyDialogState | null>(
    null,
  );
  const navigate = useNavigate();

  const onClick = (e: Event, next: string) => {
    if (navbar?.dirty() && true) {
      e.preventDefault();
      setDirtyDialog({ next });
    }
  };

  return (
    <Dialog
      id="navbar-dirty-dialog"
      open={dirtyDialog() !== null}
      onOpenChange={(open: boolean) => {
        if (!open) {
          setDirtyDialog(null);
        }
      }}
    >
      <DirtyDialog
        proceed={() => {
          const target = dirtyDialog()?.next ?? "";
          navigate(target, { resolve: false });
          navbar?.setDirty(false);
          setDirtyDialog(null);
        }}
        back={() => setDirtyDialog(null)}
      />

      {/* Hide the logo if it would shrink otherwise */}
      <a
        class={
          props.horizontal ? "hidden shrink-0 min-[486px]:block" : undefined
        }
        href={`${BASE}/`}
        onClick={(e) => onClick(e, `${BASE}/`)}
      >
        <img src={logo} width={props.horizontal ? "34" : "42"} alt="Logo" />
      </a>

      <For each={groups}>
        {([label, items]) => (
          <section class="w-full">
            <Show when={!props.horizontal}>
              <h2 class="text-muted-foreground px-3 py-1 text-xs font-semibold">
                {label}
              </h2>
            </Show>
            <For each={items}>
              {([pathname, Icon, tooltip]) => {
                const active = () => props.location.pathname === pathname;
                const style = () =>
                  active() ? navbarIconActiveStyle : navbarIconStyle;

                return (
                  <Tooltip>
                    <TooltipTrigger as="div">
                      <a href={pathname} onClick={(e) => onClick(e, pathname)}>
                        <div
                          class={`${style()} flex items-center gap-2 ${props.horizontal ? "" : "w-full justify-start"}`}
                        >
                          <Icon size={iconSize(props.horizontal)} />
                          <Show when={!props.horizontal}>
                            <span>{tooltip}</span>
                          </Show>
                        </div>
                      </a>
                    </TooltipTrigger>

                    <TooltipContent>{tooltip}</TooltipContent>
                  </Tooltip>
                );
              }}
            </For>
          </section>
        )}
      </For>
    </Dialog>
  );
}

function NavFooterItems(props: { horizontal: boolean }) {
  const systemInfo = createSystemInfoQuery();

  return (
    <>
      <Tooltip>
        <TooltipTrigger as="div">
          <SwitchThemeButton horizontal={props.horizontal} />
        </TooltipTrigger>

        <TooltipContent>
          {currentTheme() === "dark"
            ? "Switch to light mode"
            : "Switch to dark mode"}
        </TooltipContent>
      </Tooltip>

      <AuthButton iconSize={iconSize(props.horizontal)} />

      <Show when={!props.horizontal}>
        <div class="text-[9px]">
          <Version info={systemInfo.data} />
        </div>
      </Show>
    </>
  );
}

export function HorizontalNavbar(props: {
  height: number;
  location: Location;
}) {
  return (
    <nav
      style={{ height: `${props.height}px` }}
      class="border-border bg-sidebar text-sidebar-foreground flex w-screen items-center justify-between gap-2 overflow-x-auto overflow-y-hidden border-b p-2"
    >
      <NavbarItems location={props.location} horizontal={true} />

      <div class="flex items-center gap-2">
        <NavFooterItems horizontal={true} />
      </div>
    </nav>
  );
}

export function VerticalNavbar(props: { location: Location }) {
  return (
    <nav
      class={
        "border-border bg-sidebar text-sidebar-foreground flex h-dvh grow flex-col items-center justify-between gap-4 border-r py-2"
      }
    >
      <div class="flex flex-col items-center gap-4">
        <NavbarItems location={props.location} horizontal={false} />
      </div>

      <div class="flex flex-col items-center">
        <NavFooterItems horizontal={false} />
      </div>
    </nav>
  );
}

export function DirtyDialog(props: {
  back: () => void;
  proceed: () => void;
  save?: () => void;
  message?: string;
}) {
  return (
    <DialogContent
      onEscapeKeyDown={() => {
        // FIXME: escape button handler doesn't seem to work in Firefox.
        props.back();
      }}
    >
      <DialogHeader>
        <DialogTitle>Discard Changes</DialogTitle>
      </DialogHeader>

      <p>
        {props.message ??
          "The current page has pending changes. Leaving the page now will discard them. Proceed with caution."}
      </p>

      <DialogFooter>
        <div class="flex w-full justify-between">
          <Button variant="outline" onClick={props.back}>
            Back
          </Button>

          <div class="flex gap-4">
            <Show when={props.save !== undefined}>
              <Button
                variant="default"
                onClick={() => {
                  props.save?.();
                  props.proceed();
                }}
              >
                Save
              </Button>
            </Show>

            <Button variant="destructive" onClick={props.proceed}>
              {props.save !== undefined ? "Discard" : "Proceed"}
            </Button>
          </div>
        </div>
      </DialogFooter>
    </DialogContent>
  );
}

export function SwitchThemeButton(props: { horizontal: boolean }) {
  const theme = createTheme();

  return (
    <button
      type="button"
      class={navbarIconStyle}
      onClick={() => {
        const nextTheme = currentTheme() === "dark" ? "light" : "dark";
        applyResolvedTheme(nextTheme);
        $themePreference.set(nextTheme);
      }}
      aria-label={
        theme() === "dark" ? "Switch to light mode" : "Switch to dark mode"
      }
    >
      <Switch>
        <Match when={theme() === "dark"}>
          <TbOutlineSun size={iconSize(props.horizontal)} />
        </Match>

        <Match when={theme() === "light"}>
          <TbOutlineMoon size={iconSize(props.horizontal)} />
        </Match>
      </Switch>
    </button>
  );
}

function iconSize(horizontal: boolean) {
  return horizontal ? 18 : 22;
}

export const navbarIconStyle =
  "rounded-full p-2 transition-all hover:bg-accent hover:text-accent-foreground";
const navbarIconActiveStyle =
  "rounded-full bg-primary p-2 text-primary-foreground transition-all";
