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
} from "@/components/ui/sidebar";
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
export type NavbarContextT = {
  dirty: Accessor<boolean>;
  setDirty: (dirty: boolean) => void;
};
export const NavbarContext = createContext<NavbarContextT | null>(null);
export function useNavbar() {
  return useContext(NavbarContext) ?? undefined;
}

export function Navbar() {
  const location = useLocation();
  const navigate = useNavigate();
  const navbar = useNavbar();
  const [dirtyDialog, setDirtyDialog] = createSignal<string | null>(null);
  const onClick = (e: MouseEvent, next: string) => {
    if (navbar?.dirty()) {
      e.preventDefault();
      setDirtyDialog(next);
    }
  };
  return (
    <>
      <SidebarHeader>
        <a
          href={`${BASE}/`}
          onClick={(e: MouseEvent) => onClick(e, `${BASE}/`)}
        >
          <img src={logo} width="42" alt="Logo" />
        </a>
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
                      location.pathname === pathname ||
                      location.pathname.startsWith(
                        pathname.replace(/\/$/, "") + "/",
                      );
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
        <div class="flex items-center gap-2">
          <SwitchThemeButton horizontal={false} />
          <AuthButton iconSize={22} />
        </div>
        <div class="text-[9px]">
          <Version info={createSystemInfoQuery().data} />
        </div>
      </SidebarFooter>
      <Dialog
        open={dirtyDialog() !== null}
        onOpenChange={(open) => !open && setDirtyDialog(null)}
      >
        <DirtyDialog
          proceed={() => {
            navigate(dirtyDialog() ?? "", { resolve: false });
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
