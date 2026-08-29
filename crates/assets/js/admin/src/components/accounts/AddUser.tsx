import { createSignal, Show } from "solid-js";
import type { JSXElement } from "solid-js";
import { createForm } from "@tanstack/solid-form";

import { Button } from "@/components/ui/button";
import { SheetHeader, SheetTitle, SheetFooter } from "@/components/ui/sheet";
import {
  buildBoolFormField,
  buildTextFormField,
  buildSecretFormField,
  notEmptyValidator,
} from "@/components/FormFields";

import { createUser } from "@/lib/api/user";

import type { CreateUserRequest } from "@bindings/CreateUserRequest";

export function AddUser(props: {
  close: () => void;
  markDirty: () => void;
  userRefetch: () => void;
}) {
  const [error, setError] = createSignal<string>();
  const form = createForm(() => ({
    defaultValues: {
      email: "",
      password: "",
      verified: true,
      admin: false,
    } as CreateUserRequest,
    onSubmit: async ({ value }) => {
      setError(undefined);
      try {
        await createUser(value);
        props.close();
      } catch (reason) {
        setError(reason instanceof Error ? reason.message : String(reason));
      } finally {
        props.userRefetch();
      }
    },
  }));

  form.useStore((state) => {
    if (state.isDirty && !state.isSubmitted) {
      props.markDirty();
    }
  });

  return (
    <div class="overflow-x-hidden overflow-y-auto pr-1">
      <SheetHeader>
        <SheetTitle>{"Add new user"}</SheetTitle>
      </SheetHeader>

      <form
        method="dialog"
        onSubmit={(e: SubmitEvent) => {
          e.preventDefault();

          form.handleSubmit();
        }}
      >
        <Show when={error()}>
          <p class="text-destructive" role="alert">
            {error()}
          </p>
        </Show>

        <div class="flex flex-col items-start gap-4 py-4">
          <form.Field name="email" validators={notEmptyValidator()}>
            {buildTextFormField({ label: () => <L>Email</L>, type: "email" })}
          </form.Field>

          <form.Field name="password" validators={notEmptyValidator()}>
            {buildSecretFormField({
              label: () => <L>Password</L>,
            })}
          </form.Field>

          <form.Field name="admin">
            {buildBoolFormField({
              label: () => (
                <L>
                  <div class="text-right">Admin</div>
                </L>
              ),
            })}
          </form.Field>

          <form.Field name="verified">
            {buildBoolFormField({
              label: () => (
                <L>
                  <div class="text-right">Verified</div>
                </L>
              ),
            })}
          </form.Field>
        </div>

        <SheetFooter>
          <form.Subscribe
            selector={(state) => ({
              canSubmit: state.canSubmit,
              isSubmitting: state.isSubmitting,
            })}
            children={(state) => {
              return (
                <Button
                  type="submit"
                  disabled={!state().canSubmit}
                  variant="default"
                >
                  {state().isSubmitting ? "..." : "Add"}
                </Button>
              );
            }}
          />
        </SheetFooter>
      </form>
    </div>
  );
}

function L(props: { children: JSXElement }) {
  return <div class="w-32">{props.children}</div>;
}
