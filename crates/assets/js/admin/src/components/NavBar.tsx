import { For, Show } from "solid-js";
import { Location } from "@solidjs/router";
import {
  TbDatabase,
  TbEdit,
  TbUsers,
  TbChartDots3,
  TbTimeline,
  TbSettings,
} from "solid-icons/tb";

import { AuthButton } from "@/components/auth/AuthButton";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { Version } from "@/components/Version";

import { createVersionInfoQuery } from "@/lib/api/info";

import logo from "@/assets/logo_104.webp";

const BASE = import.meta.env.BASE_URL;
const options = [
  [`${BASE}/table/`, TbDatabase, "Table & View Browser"],
  [`${BASE}/editor`, TbEdit, "SQL Editor"],
  [`${BASE}/erd`, TbChartDots3, "Entity Relationship Diagram"],
  [`${BASE}/auth`, TbUsers, "User Accounts"],
  [`${BASE}/logs`, TbTimeline, "Logs & Metrics"],
  [`${BASE}/settings/`, TbSettings, "Settings"],
] as const;

const iconSize = (horizontal: boolean) => (horizontal ? 18 : 22);
export const navBarIconStyle =
  "rounded-full transition-all p-2 hover:bg-accent-200 hover:bg-opacity-50 active:scale-90";
export const navBarIconActiveStyle =
  "rounded-full transition-all p-2 bg-accent-600 text-white hover:bg-opacity-70 active:scale-90";

function NavBarItems(props: { location: Location; horizontal: boolean }) {
  return (
    <>
      <a href={`${BASE}/`}>
        <img src={logo} width={props.horizontal ? "34" : "42"} alt="Logo" />
      </a>

      <For each={options}>
        {([pathname, Icon, tooltip]) => {
          const active = () => props.location.pathname === pathname;
          const style = () =>
            active() ? navBarIconActiveStyle : navBarIconStyle;

          return (
            <Tooltip>
              <TooltipTrigger as="div">
                <a href={pathname as string}>
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
    </>
  );
}

function NavFooter(props: { horizontal: boolean }) {
  const versionInfo = createVersionInfoQuery();

  return (
    <div class="flex flex-col items-center">
      <AuthButton iconSize={iconSize(props.horizontal)} />

      <Show when={!props.horizontal}>
        <div class="text-[9px]">
          <Version info={versionInfo.data} />
        </div>
      </Show>
    </div>
  );
}

export function HorizontalNavBar(props: {
  height: number;
  location: Location;
}) {
  return (
    <nav
      style={{ height: `${props.height}px` }}
      class="flex w-screen items-center justify-between gap-4 bg-gray-100 p-2"
    >
      <NavBarItems location={props.location} horizontal={true} />

      <NavFooter horizontal={true} />
    </nav>
  );
}

export function VerticalNavBar(props: { location: Location }) {
  return (
    <div
      class={
        "flex h-dvh grow flex-col items-center justify-between gap-4 bg-gray-100 py-2"
      }
    >
      <nav class="flex flex-col items-center gap-4">
        <NavBarItems location={props.location} horizontal={false} />
      </nav>

      <NavFooter horizontal={false} />
    </div>
  );
}
