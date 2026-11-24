import type { JSXElement } from "solid-js";
import { createForm } from "@tanstack/solid-form";

import type { CreateTenantRequest } from "trailbase-bindings";

import { Button } from "@/components/ui/button";
import { SheetHeader, SheetTitle, SheetFooter } from "@/components/ui/sheet";
import {
  unsetOrNotEmptyValidator,
  buildTextFormField,
} from "@/components/FormFields";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { TextField, TextFieldLabel } from "@/components/ui/text-field";

import { createTenant } from "@/lib/api/tenant";

export function AddTenant(props: {
  close: () => void;
  markDirty: () => void;
  tenantTemplates: string[];
  tenantRefetch: () => void;
}) {
  const form = createForm(() => ({
    defaultValues: {
      template: props.tenantTemplates[0],
      name: "bar",
    } as CreateTenantRequest,
    onSubmit: async ({ value }) => {
      try {
        await createTenant(value);
        props.close();
      } finally {
        props.tenantRefetch();
      }
    },
  }));

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
        <div class="flex flex-col items-start gap-4 py-4">
          <form.Field name="template" validators={unsetOrNotEmptyValidator()}>
            {(field) => {
              return (
                <TextField class="w-full">
                  <div
                    class="grid items-center gap-x-2 gap-y-1"
                    style={{ "grid-template-columns": "auto 1fr" }}
                  >
                    <div class="w-32">
                      <TextFieldLabel>Template</TextFieldLabel>
                    </div>

                    <Select<string>
                      value={field().state.value}
                      options={props.tenantTemplates}
                      itemComponent={(props) => (
                        <SelectItem item={props.item}>
                          {props.item.rawValue}
                        </SelectItem>
                      )}
                      onChange={(value) => {
                        if (value) {
                          field().handleChange(value);
                        }
                      }}
                    >
                      <SelectTrigger>
                        <SelectValue<string>>
                          {(state) => state.selectedOption()}
                        </SelectValue>
                      </SelectTrigger>

                      <SelectContent />
                    </Select>
                  </div>
                </TextField>
              );
            }}
          </form.Field>

          <form.Field name="name" validators={unsetOrNotEmptyValidator()}>
            {buildTextFormField({ label: () => <L>Name</L> })}
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
