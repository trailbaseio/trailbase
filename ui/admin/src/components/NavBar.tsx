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

export function NavBar(props: { location: Location }) {
  return (
    <div class="grow flex flex-col justify-between items-center bg-gray-100 py-2 gap-4">
      <nav class="flex flex-col items-center gap-4">
        <a href={`${BASE}/`}>
          <img src={logo} width="52" height="52" alt="TrailBase Logo" />
        </a>

        {options.map(([pathname, icon]) => {
          const active = props.location.pathname === pathname;

          const style =
            "rounded-full hover:bg-accent-600 hover:text-white transition-all p-[10px]";
          const activeStyle = `${style} outline outline-3 outline-accent-600 text-accent-600`;

          return (
            <a href={pathname as string}>
              <div class={active ? activeStyle : style}>
                {(icon as IconTypes)({ size: 22 })}
              </div>
            </a>
          );
        })}
      </nav>

      <AuthButton />
    </div>
  );
}
