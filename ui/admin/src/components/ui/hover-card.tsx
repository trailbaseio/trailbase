import type { Component, ValidComponent } from "solid-js"
import { splitProps } from "solid-js"

import * as HoverCardPrimitive from "@kobalte/core/hover-card"
import type { PolymorphicProps } from "@kobalte/core/polymorphic"

import { cn } from "@/lib/utils"

const HoverCardTrigger = HoverCardPrimitive.Trigger

const HoverCard: Component<HoverCardPrimitive.HoverCardRootProps> = (props) => {
  return <HoverCardPrimitive.Root gutter={4} {...props} />
}

type HoverCardContentProps<T extends ValidComponent = "div"> =
  HoverCardPrimitive.HoverCardContentProps<T> & {
    class?: string | undefined
  }

const HoverCardContent = <T extends ValidComponent = "div">(
  props: PolymorphicProps<T, HoverCardContentProps<T>>
) => {
  const [local, others] = splitProps(props as HoverCardContentProps, ["class"])
  return (
    <HoverCardPrimitive.Portal>
      <HoverCardPrimitive.Content
        class={cn(
          "z-50 w-64 rounded-md border bg-popover p-4 text-popover-foreground shadow-md outline-none data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2",
          local.class
        )}
        {...others}
      />
    </HoverCardPrimitive.Portal>
  )
}

export { HoverCard, HoverCardTrigger, HoverCardContent }
