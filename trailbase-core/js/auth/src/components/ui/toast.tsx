import type { JSX, ValidComponent } from "solid-js"
import { Match, splitProps, Switch } from "solid-js"
import { Portal } from "solid-js/web"

import type { PolymorphicProps } from "@kobalte/core/polymorphic"
import * as ToastPrimitive from "@kobalte/core/toast"
import type { VariantProps } from "class-variance-authority"
import { cva } from "class-variance-authority"

import { cn } from "@/lib/utils"

const toastVariants = cva(
  "group pointer-events-auto relative flex w-full items-center justify-between space-x-4 overflow-hidden rounded-md border p-6 pr-8 shadow-lg transition-all data-[swipe=cancel]:translate-x-0 data-[swipe=end]:translate-x-[var(--kb-toast-swipe-end-x)] data-[swipe=move]:translate-x-[var(--kb-toast-swipe-move-x)] data-[swipe=move]:transition-none data-[opened]:animate-in data-[closed]:animate-out data-[swipe=end]:animate-out data-[closed]:fade-out-80 data-[closed]:slide-out-to-right-full data-[opened]:slide-in-from-top-full data-[opened]:sm:slide-in-from-bottom-full",
  {
    variants: {
      variant: {
        default: "border bg-background text-foreground",
        destructive:
          "destructive group border-destructive bg-destructive text-destructive-foreground",
        success: "success border-success-foreground bg-success text-success-foreground",
        warning: "warning border-warning-foreground bg-warning text-warning-foreground",
        error: "error border-error-foreground bg-error text-error-foreground"
      }
    },
    defaultVariants: {
      variant: "default"
    }
  }
)
type ToastVariant = NonNullable<VariantProps<typeof toastVariants>["variant"]>

type ToastListProps<T extends ValidComponent = "ol"> = ToastPrimitive.ToastListProps<T> & {
  class?: string | undefined
}

const Toaster = <T extends ValidComponent = "ol">(
  props: PolymorphicProps<T, ToastListProps<T>>
) => {
  const [local, others] = splitProps(props as ToastListProps, ["class"])
  return (
    <Portal>
      <ToastPrimitive.Region>
        <ToastPrimitive.List
          class={cn(
            "fixed top-0 z-[100] flex max-h-screen w-full flex-col-reverse gap-2 p-4 sm:bottom-0 sm:right-0 sm:top-auto sm:flex-col md:max-w-[420px]",
            local.class
          )}
          {...others}
        />
      </ToastPrimitive.Region>
    </Portal>
  )
}

type ToastRootProps<T extends ValidComponent = "li"> = ToastPrimitive.ToastRootProps<T> &
  VariantProps<typeof toastVariants> & { class?: string | undefined }

const Toast = <T extends ValidComponent = "li">(props: PolymorphicProps<T, ToastRootProps<T>>) => {
  const [local, others] = splitProps(props as ToastRootProps, ["class", "variant"])
  return (
    <ToastPrimitive.Root
      class={cn(toastVariants({ variant: local.variant }), local.class)}
      {...others}
    />
  )
}

type ToastCloseButtonProps<T extends ValidComponent = "button"> =
  ToastPrimitive.ToastCloseButtonProps<T> & { class?: string | undefined }

const ToastClose = <T extends ValidComponent = "button">(
  props: PolymorphicProps<T, ToastCloseButtonProps<T>>
) => {
  const [local, others] = splitProps(props as ToastCloseButtonProps, ["class"])
  return (
    <ToastPrimitive.CloseButton
      class={cn(
        "absolute right-2 top-2 rounded-md p-1 text-foreground/50 opacity-0 transition-opacity focus:opacity-100 focus:outline-none focus:ring-2 group-hover:opacity-100 group-[.destructive]:text-destructive-foreground group-[.error]:text-error-foreground group-[.success]:text-success-foreground group-[.warning]:text-warning-foreground",
        local.class
      )}
      {...others}
    >
      <svg
        xmlns="http://www.w3.org/2000/svg"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
        class="size-4"
      >
        <path d="M18 6l-12 12" />
        <path d="M6 6l12 12" />
      </svg>
    </ToastPrimitive.CloseButton>
  )
}

type ToastTitleProps<T extends ValidComponent = "div"> = ToastPrimitive.ToastTitleProps<T> & {
  class?: string | undefined
}

const ToastTitle = <T extends ValidComponent = "div">(
  props: PolymorphicProps<T, ToastTitleProps<T>>
) => {
  const [local, others] = splitProps(props as ToastTitleProps, ["class"])
  return <ToastPrimitive.Title class={cn("text-sm font-semibold", local.class)} {...others} />
}

type ToastDescriptionProps<T extends ValidComponent = "div"> =
  ToastPrimitive.ToastDescriptionProps<T> & { class?: string | undefined }

const ToastDescription = <T extends ValidComponent = "div">(
  props: PolymorphicProps<T, ToastDescriptionProps<T>>
) => {
  const [local, others] = splitProps(props as ToastDescriptionProps, ["class"])
  return <ToastPrimitive.Description class={cn("text-sm opacity-90", local.class)} {...others} />
}

function showToast(props: {
  title?: JSX.Element
  description?: JSX.Element
  variant?: ToastVariant
  duration?: number
}) {
  ToastPrimitive.toaster.show((data) => (
    <Toast toastId={data.toastId} variant={props.variant} duration={props.duration}>
      <div class="grid gap-1">
        {props.title && <ToastTitle>{props.title}</ToastTitle>}
        {props.description && <ToastDescription>{props.description}</ToastDescription>}
      </div>
      <ToastClose />
    </Toast>
  ))
}

function showToastPromise<T, U>(
  promise: Promise<T> | (() => Promise<T>),
  options: {
    loading?: JSX.Element
    success?: (data: T) => JSX.Element
    error?: (error: U) => JSX.Element
    duration?: number
  }
) {
  const variant: { [key in ToastPrimitive.ToastPromiseState]: ToastVariant } = {
    pending: "default",
    fulfilled: "success",
    rejected: "error"
  }
  return ToastPrimitive.toaster.promise<T, U>(promise, (props) => (
    <Toast toastId={props.toastId} variant={variant[props.state]} duration={options.duration}>
      <Switch>
        <Match when={props.state === "pending"}>{options.loading}</Match>
        <Match when={props.state === "fulfilled"}>{options.success?.(props.data!)}</Match>
        <Match when={props.state === "rejected"}>{options.error?.(props.error!)}</Match>
      </Switch>
    </Toast>
  ))
}

export { Toaster, Toast, ToastClose, ToastTitle, ToastDescription, showToast, showToastPromise }
