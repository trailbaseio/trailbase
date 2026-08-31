import { createContext, createSignal, useContext, For, Show } from "solid-js";
import type { Accessor } from "solid-js";
import { useNavigate, useLocation } from "@solidjs/router";
import {
  TbOutlineDatabase,
  TbOutlineEdit,
  TbOutlineUsers,
  TbOutlineChartDots3,
  TbOutlineTimeline,
  TbOutlineSettings,
  TbOutlinePackage,
  TbOutlineApi,
  TbOutlineMoon,
  TbOutlineSun,
  TbOutlineLayoutSidebarLeftCollapse,
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
  createTheme,
  currentTheme,
  applyResolvedTheme,
  $themePreference,
} from "@/lib/theme";
import { Version } from "@/components/Version";
import { createSystemInfoQuery } from "@/lib/api/info";
import {
  SidebarHeader,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuItem,
  SidebarMenuButton,
  useSidebar,
} from "@/components/ui/sidebar";
import logo from "@/assets/favicon.svg";

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
export type NavbarContextT = {
  dirty: Accessor<boolean>;
  setDirty: (dirty: boolean) => void;
};
export const NavbarContext = createContext<NavbarContextT | null>(null);
export function useNavbar() {
  return useContext(NavbarContext) ?? undefined;
}

/** Match a route exactly or beneath it, ignoring trailing slashes. */
export function isPathActive(current: string, target: string): boolean {
  const normalize = (path: string) => path.replace(/\/+$/, "") || "/";
  const currentPath = normalize(current);
  const targetPath = normalize(target);
  return (
    currentPath === targetPath ||
    (targetPath !== "/" && currentPath.startsWith(`${targetPath}/`))
  );
}

export function Navbar() {
  const location = useLocation();
  const navigate = useNavigate();
  const navbar = useNavbar();
  const { isMobile, setOpenMobile, state, toggleSidebar } = useSidebar();
  const sidebarActionLabel = () =>
    isMobile()
      ? "Close navigation"
      : state() === "collapsed"
        ? "Expand sidebar"
        : "Collapse sidebar";
  const [dirtyDialog, setDirtyDialog] = createSignal<string | null>(null);
  const onClick = (e: MouseEvent, next: string) => {
    if (navbar?.dirty()) {
      e.preventDefault();
      setDirtyDialog(next);
    } else {
      setOpenMobile(false);
    }
  };
  return (
    <>
      <SidebarHeader>
        <a
          class="flex h-9 items-center gap-2 px-2 group-data-[collapsible=icon]:justify-center group-data-[collapsible=icon]:px-0"
          href={`${BASE}/`}
          onClick={(e: MouseEvent) => onClick(e, `${BASE}/`)}
        >
          <img class="size-7 shrink-0" src={logo} alt="" />
          <span class="truncate text-base font-semibold group-data-[collapsible=icon]:hidden">
            TrailBase
          </span>
        </a>
        <Button
          variant="ghost"
          class="w-full justify-start px-2 group-data-[collapsible=icon]:mx-auto group-data-[collapsible=icon]:size-8 group-data-[collapsible=icon]:justify-center group-data-[collapsible=icon]:px-0"
          aria-label={sidebarActionLabel()}
          title={sidebarActionLabel()}
          onClick={toggleSidebar}
        >
          <TbOutlineLayoutSidebarLeftCollapse
            class={`transition-transform ${state() === "collapsed" ? "rotate-180" : ""}`}
          />
          <span class="group-data-[collapsible=icon]:hidden">
            {sidebarActionLabel()}
          </span>
        </Button>
      </SidebarHeader>
      <SidebarContent>
        <For each={groups}>
          {([label, items]) => (
            <SidebarGroup>
              <SidebarGroupLabel>{label}</SidebarGroupLabel>
              <SidebarMenu>
                <For each={items}>
                  {([pathname, Icon, text]) => {
                    const active = () =>
                      isPathActive(location.pathname, pathname);
                    return (
                      <SidebarMenuItem>
                        <SidebarMenuButton
                          as="a"
                          href={pathname}
                          isActive={active()}
                          tooltip={text}
                          onClick={(e: MouseEvent) => onClick(e, pathname)}
                        >
                          <Icon />
                          <span>{text}</span>
                        </SidebarMenuButton>
                      </SidebarMenuItem>
                    );
                  }}
                </For>
              </SidebarMenu>
            </SidebarGroup>
          )}
        </For>
      </SidebarContent>
      <SidebarFooter>
        <div class="flex min-w-0 flex-col gap-1 group-data-[collapsible=icon]:items-center">
          <div class="flex min-w-0 items-center gap-2 group-data-[collapsible=icon]:flex-col">
            <SwitchThemeButton horizontal={false} />
            <AuthButton iconSize={22} />
          </div>
          <div class="truncate text-[9px] group-data-[collapsible=icon]:hidden">
            <Version info={createSystemInfoQuery().data} />
          </div>
        </div>
      </SidebarFooter>
      <Dialog
        open={dirtyDialog() !== null}
        onOpenChange={(open) => !open && setDirtyDialog(null)}
      >
        <DirtyDialog
          proceed={() => {
            navigate(dirtyDialog() ?? "", { resolve: false });
            setOpenMobile(false);
            navbar?.setDirty(false);
            setDirtyDialog(null);
          }}
          back={() => setDirtyDialog(null)}
        />
      </Dialog>
    </>
  );
}

export function DirtyDialog(props: {
  back: () => void;
  proceed: () => void;
  save?: () => void;
  message?: string;
}) {
  return (
    <DialogContent onEscapeKeyDown={props.back}>
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
            <Show when={props.save}>
              <Button
                onClick={() => {
                  props.save?.();
                  props.proceed();
                }}
              >
                Save
              </Button>
            </Show>
            <Button variant="destructive" onClick={props.proceed}>
              {props.save ? "Discard" : "Proceed"}
            </Button>
          </div>
        </div>
      </DialogFooter>
    </DialogContent>
  );
}

export function SwitchThemeButton(_props: { horizontal: boolean }) {
  const theme = createTheme();
  return (
    <button
      type="button"
      class="hover:bg-accent rounded-full p-2"
      onClick={() => {
        const next = currentTheme() === "dark" ? "light" : "dark";
        applyResolvedTheme(next);
        $themePreference.set(next);
      }}
      aria-label="Switch theme"
    >
      {theme() === "dark" ? (
        <TbOutlineSun size={22} />
      ) : (
        <TbOutlineMoon size={22} />
      )}
    </button>
  );
}
