import { Show, type JSX } from "solid-js";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";

export const iconButtonStyle =
  "grid items-center justify-center size-[32px] p-1 rounded hover:bg-gray-200";

export function IconButton(props: {
  children: JSX.Element;
  onClick: () => void;
  tooltip?: string;
}) {
  const Button = () => (
    <button class={iconButtonStyle} onClick={props.onClick}>
      {props.children}
    </button>
  );

  return (
    <Show when={props.tooltip} fallback={<Button />}>
      <Tooltip>
        <TooltipTrigger as="div">
          <Button />
        </TooltipTrigger>

        <TooltipContent>{props.tooltip}</TooltipContent>
      </Tooltip>
    </Show>
  );
}
