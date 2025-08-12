/* eslint-disable solid/reactivity */
/* eslint-disable @typescript-eslint/no-explicit-any */
import { createSignal, type Setter, type JSX } from "solid-js";
import { describe, test, expect } from "vitest";
import { render } from "@solidjs/testing-library";
import userEvent from "@testing-library/user-event";
import { createForm, type DeepKeys } from "@tanstack/solid-form";

import {
  buildTextFormField,
  buildOptionalTextFormField,
  buildNullableTextFormField,
  type FieldApiT,
} from "@/components/FormFields";

function getCheckbox(dom: any): HTMLInputElement {
  // NOTE: The solid-ui Checkbox component wraps the input in a parent div.
  const div = dom.getByTestId("toggle") as HTMLDivElement;
  return div.children[0] as HTMLInputElement;
}

interface MyForm {
  required: string;
  optional: string | undefined;
  nullable: string | null;
  optionalNullable: string | null | undefined;
}

function Form(props: {
  name: DeepKeys<MyForm>;
  setForm: Setter<MyForm | undefined>;
  defaultValue?: MyForm;
  field: (field: () => FieldApiT<any>) => JSX.Element;
}) {
  const form = createForm(() => ({
    defaultValues:
      props.defaultValue ??
      ({
        required: "default",
        nullable: null,
      } as MyForm),
    onSubmit: async ({ value }: { value: MyForm }) => props.setForm(value),
  }));

  return (
    <form
      method="dialog"
      onSubmit={(e: SubmitEvent) => {
        e.preventDefault();
        form.handleSubmit();
      }}
    >
      <form.Field name={props.name}>{props.field}</form.Field>

      <form.Subscribe>
        <button type="submit" data-testid="sub">
          Submit
        </button>
      </form.Subscribe>
    </form>
  );
}

describe("required form fields", () => {
  test("test required form", async () => {
    const user = userEvent.setup();
    const [form, setForm] = createSignal<MyForm | undefined>();

    const result = render(() => (
      <Form
        name="required"
        setForm={setForm}
        field={buildTextFormField({ label: () => "required" })}
      />
    ));

    {
      const input: HTMLInputElement = result.getByTestId("input");
      await user.type(input, " test");

      await user.click(result.getByTestId("sub"));

      expect(form()!.required).toBe("default test");
    }

    {
      const input: HTMLInputElement = result.getByTestId("input");
      await user.clear(input);
      await user.click(result.getByTestId("sub"));
      expect(form()!.required).toBe("");
    }
  });
});

describe("nullable form fields", () => {
  test("set", async () => {
    const user = userEvent.setup();
    const [form, setForm] = createSignal<MyForm | undefined>();

    const dom = render(() => (
      <Form
        name="nullable"
        setForm={setForm}
        field={buildNullableTextFormField({ label: () => "nullable" })}
      />
    ));

    const input: HTMLInputElement = dom.getByTestId("input");
    expect(input.disabled).toBe(true);

    // The input field is disabled to to it's initial value being null.
    await user.click(getCheckbox(dom));

    await user.type(input, "nullable");
    expect(input.value).toBe("nullable");

    await user.click(dom.getByTestId("sub"));

    const value = form()!;
    expect(value.nullable).toBe("nullable");
  });

  test("set and unset", async () => {
    const user = userEvent.setup();
    const [form, setForm] = createSignal<MyForm | undefined>();

    const dom = render(() => (
      <Form
        name="nullable"
        setForm={setForm}
        field={buildNullableTextFormField({ label: () => "nullable" })}
      />
    ));

    const input: HTMLInputElement = dom.getByTestId("input");
    expect(input.disabled).toBe(true);

    // The input field is disabled to to it's initial value being null.
    await user.click(getCheckbox(dom));

    await user.type(input, "nullable");
    expect(input.value).toBe("nullable");

    // Click again to unset the value.
    await user.click(getCheckbox(dom));

    await user.click(dom.getByTestId("sub"));

    const value = form()!;
    expect(value.nullable).toBe(null);
  });
});

describe("optional form fields", () => {
  test("set", async () => {
    const user = userEvent.setup();
    const [form, setForm] = createSignal<MyForm | undefined>();

    const dom = render(() => (
      <Form
        name="optional"
        setForm={setForm}
        field={buildOptionalTextFormField({ label: () => "optional" })}
      />
    ));

    const input: HTMLInputElement = dom.getByTestId("input");
    expect(input.disabled).toBe(false);

    await user.type(input, "optional");
    expect(input.value, "optional");

    await user.click(dom.getByTestId("sub"));

    const value = form()!;
    expect(value.optional).toBe("optional");
  });

  test("set and unset", async () => {
    const user = userEvent.setup();
    const [form, setForm] = createSignal<MyForm | undefined>();

    const result = render(() => (
      <Form
        name="optional"
        setForm={setForm}
        field={buildOptionalTextFormField({ label: () => "optional" })}
      />
    ));

    const input: HTMLInputElement = result.getByTestId("input");
    expect(input.value).toBe("");

    await user.type(input, "optional");
    await user.clear(input);

    await user.click(result.getByTestId("sub"));

    const value = form()!;
    expect(value.optional).toBeUndefined();
  });
});
