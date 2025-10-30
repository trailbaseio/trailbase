import { Show, splitProps, type ValidComponent } from "solid-js";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { Button } from "@/components/ui/button";
import * as ButtonPrimitive from "@kobalte/core/button";
import { type PolymorphicProps } from "@kobalte/core/polymorphic";

type ButtonProps<T extends ValidComponent = "button"> =
  ButtonPrimitive.ButtonRootProps<T> & {
    class?: string | undefined;
    tooltip?: string | undefined;
  };

export function IconButton<T extends ValidComponent = "button">(
  props: PolymorphicProps<T, ButtonProps<T>>,
) {
  const [local, others] = splitProps(props as ButtonProps, ["tooltip"]);

  const B = () => <Button variant="ghost" size="icon" {...others} />;

  return (
    <Show when={local.tooltip} fallback={<B />}>
      <Tooltip>
        <TooltipTrigger as="div">
          <B />
        </TooltipTrigger>

        <TooltipContent>{props.tooltip}</TooltipContent>
      </Tooltip>
    </Show>
  );
}
