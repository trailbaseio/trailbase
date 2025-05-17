import { For } from "solid-js";
import { Location } from "@solidjs/router";
import {
  TbDatabase,
  TbEdit,
  TbUsers,
  TbChartDots3,
  TbTimeline,
  TbSettings,
} from "solid-icons/tb";
import { IconTypes } from "solid-icons";

import { AuthButton } from "@/components/auth/AuthButton";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";

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

export const navBarIconSize = 22;
export const navBarIconStyle =
  "rounded-full transition-all p-[10px] hover:bg-accent-200 hover:bg-opacity-50 active:scale-90";
export const navBarIconActiveStyle =
  "rounded-full transition-all p-[10px] bg-accent-600 text-white hover:bg-opacity-70 active:scale-90";

export function NavBar(props: { location: Location }) {
  return (
    <div class="flex grow flex-col items-center justify-between gap-4 bg-gray-100 py-2">
      <nav class="flex flex-col items-center gap-4">
        <a href={`${BASE}/`}>
          <img src={logo} width="42" height="42" alt="TrailBase Logo" />
        </a>

        <For each={options}>
          {([pathname, icon, tooltip]) => {
            const active = () => props.location.pathname === pathname;

            return (
              <Tooltip>
                <TooltipTrigger as="div">
                  <a href={pathname as string}>
                    <div
                      class={active() ? navBarIconActiveStyle : navBarIconStyle}
                    >
                      {(icon as IconTypes)({ size: navBarIconSize })}
                    </div>
                  </a>
                </TooltipTrigger>

                <TooltipContent>{tooltip}</TooltipContent>
              </Tooltip>
            );
          }}
        </For>
      </nav>

      <AuthButton />
    </div>
  );
}
