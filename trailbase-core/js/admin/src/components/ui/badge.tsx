import type { Component, ComponentProps } from "solid-js"
import { splitProps } from "solid-js"
import * as ButtonPrimitive from "@kobalte/core/button";

import type { VariantProps } from "class-variance-authority"
import { cva } from "class-variance-authority"

import { cn } from "@/lib/utils"

const badgeVariants = cva(
  "inline-flex items-center border px-2.5 py-0.5 text-xs font-semibold transition-colors focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2",
  {
    variants: {
      variant: {
        default: "border-transparent bg-primary text-primary-foreground hover:bg-primary/80",
        secondary:
          "border-transparent bg-secondary text-secondary-foreground hover:bg-secondary/80",
        destructive:
          "border-transparent bg-destructive text-destructive-foreground hover:bg-destructive/80",
        outline: "text-foreground"
      }
    },
    defaultVariants: {
      variant: "default"
    }
  }
)

type BadgeProps = ComponentProps<"div"> &
  VariantProps<typeof badgeVariants> & {
    round?: boolean
  }

const Badge: Component<BadgeProps> = (props) => {
  const [local, others] = splitProps(props, ["class", "variant", "round"])
  return (
    <div
      class={cn(
        badgeVariants({ variant: local.variant }),
        local.round ? "rounded-full" : "rounded-md",
        local.class
      )}
      {...others}
    />
  )
}

type ButtonBadgeProps = ComponentProps<"button"> &
  VariantProps<typeof badgeVariants> & {
    round?: boolean
  }

const ButtonBadge: Component<BadgeProps> = (props) => {
  const [local, others] = splitProps(props, ["class", "variant", "round"])
  return (
    <ButtonPrimitive.Root
      class={cn(
        badgeVariants({ variant: local.variant }),
        local.round ? "rounded-full" : "rounded-md",
        local.class
      )}
      {...others}
    />
  )
}

export type { BadgeProps, ButtonBadgeProps }
export { Badge, ButtonBadge, badgeVariants }
