import { Match, Switch } from "solid-js";
import { TbOutlineMoon, TbOutlineSun } from "solid-icons/tb";

import { cn } from "@/lib/utils";
import { currentTheme, applyResolvedTheme, createTheme } from "@/lib/theme";

export function SwitchThemeButton() {
  const theme = createTheme();

  return (
    <button
      type="button"
      class={cn(ICON_STYLE)}
      onClick={() => {
        applyResolvedTheme(currentTheme() === "dark" ? "light" : "dark");
      }}
      aria-label={
        theme() === "dark" ? "Switch to light mode" : "Switch to dark mode"
      }
    >
      <Switch>
        <Match when={theme() === "dark"}>
          <TbOutlineSun />
        </Match>

        <Match when={theme() === "light"}>
          <TbOutlineMoon />
        </Match>
      </Switch>
    </button>
  );
}

const ICON_STYLE = [
  "inline-flex",
  "items-center",
  "justify-center",
  "rounded-md",
  "p-2",
  "hover:text-primary-foreground",
  "hover:bg-primary/90",
  "data-[expanded]:text-primary-foreground",
  "data-[expanded]:bg-primary/90",
];
