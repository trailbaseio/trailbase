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
import { createTheme, currentTheme, applyResolvedTheme } from "@/lib/theme";

import logo from "@/assets/logo_104.webp";

const BASE = import.meta.env.BASE_URL;
const options = [
  [`${BASE}/table/`, TbOutlineDatabase, "Table & View Browser"],
  [`${BASE}/editor`, TbOutlineEdit, "SQL Editor"],
  [`${BASE}/erd`, TbOutlineChartDots3, "Entity Relationship Diagram"],
  [`${BASE}/auth`, TbOutlineUsers, "User Accounts"],
  [`${BASE}/wasm-modules`, TbOutlinePackage, "WASM Modules"],
  [`${BASE}/logs`, TbOutlineTimeline, "Logs & Metrics"],
  [`${BASE}/settings/`, TbOutlineSettings, "Settings"],
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

      <a href={`${BASE}/`} onClick={(e) => onClick(e, `${BASE}/`)}>
        <img src={logo} width={props.horizontal ? "34" : "42"} alt="Logo" />
      </a>

      <For each={options}>
        {([pathname, Icon, tooltip]) => {
          const active = () => props.location.pathname === pathname;
          const style = () =>
            active() ? navbarIconActiveStyle : navbarIconStyle;

          return (
            <Tooltip>
              <TooltipTrigger as="div">
                <a href={pathname} onClick={(e) => onClick(e, pathname)}>
                  <div class={style()}>
                    <Icon size={iconSize(props.horizontal)} />
                  </div>
                </a>
              </TooltipTrigger>

              <TooltipContent>{tooltip}</TooltipContent>
            </Tooltip>
          );
        }}
      </For>
    </Dialog>
  );
}

function NavFooter(props: { horizontal: boolean }) {
  const systemInfo = createSystemInfoQuery();

  return (
    <div
      class={
        props.horizontal
          ? "flex items-center gap-4"
          : "flex flex-col items-center"
      }
    >
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
    </div>
  );
}

export function HorizontalNavbar(props: {
  height: number;
  location: Location;
}) {
  return (
    <nav
      style={{ height: `${props.height}px` }}
      class="border-border bg-sidebar text-sidebar-foreground flex w-screen items-center justify-between gap-2 border-b p-2"
    >
      <NavbarItems location={props.location} horizontal={true} />

      <NavFooter horizontal={true} />
    </nav>
  );
}

export function VerticalNavbar(props: { location: Location }) {
  return (
    <div
      class={
        "border-border bg-sidebar text-sidebar-foreground flex h-dvh grow flex-col items-center justify-between gap-4 border-r py-2"
      }
    >
      <nav class="flex flex-col items-center gap-4">
        <NavbarItems location={props.location} horizontal={false} />
      </nav>

      <NavFooter horizontal={false} />
    </div>
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
        applyResolvedTheme(currentTheme() === "dark" ? "light" : "dark");
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
  "rounded-full p-2 transition-all hover:bg-accent hover:text-accent-foreground active:scale-90";
const navbarIconActiveStyle =
  "rounded-full bg-primary p-2 text-primary-foreground transition-all active:scale-90";
