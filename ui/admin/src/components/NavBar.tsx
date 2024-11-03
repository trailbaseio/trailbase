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

import logo from "@assets/logo_104.webp";

const BASE = "/_/admin";
const options = [
  [`${BASE}/tables`, TbDatabase],
  [`${BASE}/editor`, TbEdit],
  [`${BASE}/auth`, TbUsers],
  [`${BASE}/logs`, TbTimeline],
  [`${BASE}/settings`, TbSettings],
];

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
          <img src={logo} width="52" height="52" alt="TrailBase Logo" />
        </a>

        {options.map(([pathname, icon]) => {
          const active = () => props.location.pathname === pathname;

          return (
            <a href={pathname as string}>
              <div class={active() ? navBarIconActiveStyle : navBarIconStyle}>
                {(icon as IconTypes)({ size: navBarIconSize })}
              </div>
            </a>
          );
        })}
      </nav>

      <AuthButton />
    </div>
  );
}
