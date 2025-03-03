import { Show, splitProps, type ValidComponent } from "solid-js";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import * as ButtonPrimitive from "@kobalte/core/button";
import { type PolymorphicProps } from "@kobalte/core/polymorphic";
import { cn } from "@/lib/utils";

export const iconButtonStyle =
  "grid items-center justify-center size-[32px] p-1 rounded hover:bg-gray-200 active:scale-90";

type ButtonProps<T extends ValidComponent = "button"> =
  ButtonPrimitive.ButtonRootProps<T> & {
    class?: string | undefined;
    tooltip?: string | undefined;
  };

export function IconButton<T extends ValidComponent = "button">(
  props: PolymorphicProps<T, ButtonProps<T>>,
) {
  const [local, others] = splitProps(props as ButtonProps, [
    "tooltip",
    "class",
  ]);

  const Button = () => (
    <ButtonPrimitive.Root
      class={cn(iconButtonStyle, local.class)}
      {...others}
    />
  );

  return (
    <Show when={local.tooltip} fallback={<Button />}>
      <Tooltip>
        <TooltipTrigger as="div">
          <Button />
        </TooltipTrigger>

        <TooltipContent>{props.tooltip}</TooltipContent>
      </Tooltip>
    </Show>
  );
}
