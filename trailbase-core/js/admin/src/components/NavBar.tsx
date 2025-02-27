import { Location } from "@solidjs/router";
import {
  TbDatabase,
  TbEdit,
  TbUsers,
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

const BASE = "/_/admin";
const options = [
  [`${BASE}/tables`, TbDatabase, "Table & View Browser"],
  [`${BASE}/editor`, TbEdit, "SQL Editor"],
  [`${BASE}/auth`, TbUsers, "User Accounts"],
  [`${BASE}/logs`, TbTimeline, "Logs & Metrics"],
  [`${BASE}/settings`, TbSettings, "Settings"],
] as const;

export const navBarIconSize = 22;
export const navBarIconStyle =
  "rounded-full hover:bg-accent-200 hover:bg-opacity-50 transition-all p-[10px]";
export const navBarIconActiveStyle =
  "rounded-full transition-all p-[10px] bg-accent-600 text-white hover:bg-opacity-70";

export function NavBar(props: { location: Location }) {
  return (
    <div class="grow flex flex-col justify-between items-center bg-gray-100 py-2 gap-4">
      <nav class="flex flex-col items-center gap-4">
        <a href={`${BASE}/`}>
          <img src={logo} width="42" height="42" alt="TrailBase Logo" />
        </a>

        {options.map(([pathname, icon, tooltip]) => {
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
        })}
      </nav>

      <AuthButton />
    </div>
  );
}
