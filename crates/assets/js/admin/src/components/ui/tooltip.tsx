import { splitProps, type ValidComponent, type Component } from "solid-js"

import type { PolymorphicProps } from "@kobalte/core/polymorphic"
import * as TooltipPrimitive from "@kobalte/core/tooltip"

import { cn } from "@/lib/utils"

const Tooltip: Component<TooltipPrimitive.TooltipRootProps> = (props) => {
  return <TooltipPrimitive.Root gutter={4} {...props} />
}

type TooltipTriggerProps<T extends ValidComponent = "button"> = TooltipPrimitive.TooltipTriggerProps<T>

// const TooltipTrigger = TooltipPrimitive.Trigger
const TooltipTrigger = <T extends ValidComponent = "button">(
  props: PolymorphicProps<T, TooltipTriggerProps<T>>
) => {
  // NOTE: We're setting `type="button"` to prevent the trigger from triggering form submissions.
  return (
    <TooltipPrimitive.Trigger type="button" {...props} />
  )
}

type TooltipContentProps<T extends ValidComponent = "div"> =
  TooltipPrimitive.TooltipContentProps<T> & { class?: string | undefined }

const TooltipContent = <T extends ValidComponent = "div">(
  props: PolymorphicProps<T, TooltipContentProps<T>>
) => {
  const [local, others] = splitProps(props as TooltipContentProps, ["class"])
  return (
    <TooltipPrimitive.Portal>
      <TooltipPrimitive.Content
        class={cn(
          "z-50 origin-[var(--kb-popover-content-transform-origin)] overflow-hidden rounded-md border bg-popover px-3 py-1.5 text-sm text-popover-foreground shadow-md animate-in fade-in-0 zoom-in-95",
          local.class
        )}
        {...others}
      />
    </TooltipPrimitive.Portal>
  )
}

export { Tooltip, TooltipTrigger, TooltipContent }
