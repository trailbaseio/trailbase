import type { JSXElement } from "solid-js";
import { createForm } from "@tanstack/solid-form";

import { Button } from "@/components/ui/button";
import { SheetHeader, SheetTitle, SheetFooter } from "@/components/ui/sheet";

import {
  buildBoolFormField,
  buildTextFormField,
  notEmptyValidator,
} from "@/components/FormFields";
import type { CreateUserRequest } from "@/lib/bindings";
import { createUser } from "@/lib/user";

export function AddUser(props: {
  close: () => void;
  markDirty: () => void;
  userRefetch: () => void;
}) {
  const form = createForm<CreateUserRequest>(() => ({
    defaultValues: {
      email: "",
      password: "",
      verified: true,
      admin: false,
    },
    onSubmit: async ({ value }) => {
      createUser(value)
        .then(() => {
          props.userRefetch();
          props.close();
        })
        .catch(console.error);
    },
  }));

  return (
    <div class="overflow-y-auto overflow-x-hidden pr-1">
      <SheetHeader>
        <SheetTitle>{"Add new user"}</SheetTitle>
      </SheetHeader>

      <form
        onSubmit={(e) => {
          e.preventDefault();
          e.stopPropagation();
          form.handleSubmit();
        }}
      >
        <div class="flex flex-col items-start gap-4 py-4">
          <form.Field name="email" validators={notEmptyValidator()}>
            {buildTextFormField({ label: () => <L>E-mail</L>, type: "email" })}
          </form.Field>

          <form.Field name="password" validators={notEmptyValidator()}>
            {buildTextFormField({
              label: () => <L>Password</L>,
              type: "password",
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