import { Match, Switch } from "solid-js";
import { TbOutlineEye, TbOutlineTable, TbOutlineWand } from "solid-icons/tb";

import type { TableType } from "@/lib/schema";

export function SchemaIcon(props: { type: TableType }) {
  return (
    <Switch>
      <Match when={props.type === "view"}>
        <TbOutlineEye />
      </Match>

      <Match when={props.type === "virtualTable"}>
        <TbOutlineWand />
      </Match>

      <Match when={props.type === "table"}>
        <TbOutlineTable />
      </Match>
    </Switch>
  );
}
