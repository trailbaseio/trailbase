/* eslint-disable solid/reactivity */
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

const user = userEvent.setup();

describe("form fields", () => {
  interface MyForm {
    required: string;
    optional: string | undefined;
    nullable: string | null;
    optionalNullable: string | null | undefined;
  }

  function newMyForm(
    setter: Setter<MyForm | undefined>,
    defaultValue?: MyForm,
  ) {
    const form = createForm(() => ({
      defaultValues:
        defaultValue ??
        ({
          required: "default",
          nullable: null,
        } as MyForm),
      onSubmit: async ({ value }: { value: MyForm }) => setter(value),
    }));

    return form;
  }

  function Form(props: {
    name: DeepKeys<MyForm>;
    setForm: Setter<MyForm | undefined>;
    defaultValue?: MyForm;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    field: (field: () => FieldApiT<any>) => JSX.Element;
  }) {
    const form = newMyForm(props.setForm, props.defaultValue);

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

  test("test required form", async () => {
    const [form, setForm] = createSignal<MyForm | undefined>();

    const result = render(() => (
      <Form
        name="required"
        setForm={setForm}
        field={buildTextFormField({ label: () => "required" })}
      />
    ));

    const input: HTMLInputElement = result.getByTestId("input");
    await user.type(input, " test");

    await user.click(result.getByTestId("sub"));

    const value = form()!;
    expect(value.required).toBe("default test");
  });

  describe("nullable", () => {
    test("set", async () => {
      const [form, setForm] = createSignal<MyForm | undefined>();

      const result = render(() => (
        <Form
          name="nullable"
          setForm={setForm}
          field={buildNullableTextFormField({ label: () => "nullable" })}
        />
      ));

      const input: HTMLInputElement = result.getByTestId("input");
      expect(input.disabled);

      // The input field is disabled to to it's initial value being null.
      // NOTE: The solid-ui Checkbox component wraps the input in a parent div.
      const toggle = result.getByTestId("toggle")
        .firstChild! as HTMLInputElement;
      await user.click(toggle);
      expect(toggle.value);

      await user.type(input, "nullable");
      expect(input.value, "nullable");

      await user.click(result.getByTestId("sub"));

      const value = form()!;
      expect(value.nullable).toBe("nullable");
    });

    test("set and unset", async () => {
      const [form, setForm] = createSignal<MyForm | undefined>();

      const result = render(() => (
        <Form
          name="nullable"
          setForm={setForm}
          field={buildNullableTextFormField({ label: () => "nullable" })}
        />
      ));

      const input: HTMLInputElement = result.getByTestId("input");
      expect(input.disabled);

      // The input field is disabled to to it's initial value being null.
      // NOTE: The solid-ui Checkbox component wraps the input in a parent div.
      const toggle = result.getByTestId("toggle")
        .firstChild! as HTMLInputElement;
      await user.click(toggle);
      expect(toggle.value);

      await user.type(input, "nullable");
      expect(input.value, "nullable");

      await user.click(toggle);
      expect(!toggle.value);

      await user.click(result.getByTestId("sub"));

      const value = form()!;
      expect(value.nullable).toBe(null);
    });
  });

  describe("optional nullable", () => {
    test("set", async () => {
      const [form, setForm] = createSignal<MyForm | undefined>();

      const result = render(() => (
        <Form
          name="optionalNullable"
          setForm={setForm}
          field={buildNullableTextFormField({ label: () => "optional" })}
        />
      ));

      const input: HTMLInputElement = result.getByTestId("input");
      expect(input.disabled);

      // The input field is disabled to to it's initial value being null.
      // NOTE: The solid-ui Checkbox component wraps the input in a parent div.
      const toggle = result.getByTestId("toggle")
        .firstChild! as HTMLInputElement;
      await user.click(toggle);
      expect(toggle.value);

      await user.type(input, "optional");
      expect(input.value, "optional");

      await user.click(result.getByTestId("sub"));

      const value = form()!;
      expect(value.optionalNullable).toBe("optional");
    });

    test("set and unset", async () => {
      const [form, setForm] = createSignal<MyForm | undefined>();

      const result = render(() => (
        <Form
          name="optionalNullable"
          setForm={setForm}
          field={buildNullableTextFormField({ label: () => "optional" })}
        />
      ));

      const input: HTMLInputElement = result.getByTestId("input");
      expect(input.disabled);

      // The input field is disabled to to it's initial value being null.
      // NOTE: The solid-ui Checkbox component wraps the input in a parent div.
      const toggle = result.getByTestId("toggle")
        .firstChild! as HTMLInputElement;
      await user.click(toggle);
      expect(toggle.value);

      await user.type(input, "optional");
      expect(input.value, "optional");

      await user.click(toggle);
      expect(!toggle.value);

      await user.click(result.getByTestId("sub"));

      const value = form()!;
      expect(value.optionalNullable).toBe(null);
    });
  });

  describe("optional", () => {
    test("set", async () => {
      const [form, setForm] = createSignal<MyForm | undefined>();

      const result = render(() => (
        <Form
          name="optional"
          setForm={setForm}
          field={buildOptionalTextFormField({ label: () => "optional" })}
        />
      ));

      const input: HTMLInputElement = result.getByTestId("input");
      expect(input.disabled);

      // The input field is disabled to to it's initial value being null.
      // NOTE: The solid-ui Checkbox component wraps the input in a parent div.
      const toggle = result.getByTestId("toggle")
        .firstChild! as HTMLInputElement;
      await user.click(toggle);
      expect(toggle.value);

      await user.type(input, "optional");
      expect(input.value, "optional");

      await user.click(result.getByTestId("sub"));

      const value = form()!;
      expect(value.optional).toBe("optional");
    });

    test("set and unset", async () => {
      const [form, setForm] = createSignal<MyForm | undefined>();

      const result = render(() => (
        <Form
          name="optional"
          setForm={setForm}
          field={buildOptionalTextFormField({ label: () => "optional" })}
        />
      ));

      const input: HTMLInputElement = result.getByTestId("input");
      expect(input.disabled);

      // The input field is disabled to to it's initial value being null.
      // NOTE: The solid-ui Checkbox component wraps the input in a parent div.
      const toggle = result.getByTestId("toggle")
        .firstChild! as HTMLInputElement;
      await user.click(toggle);
      expect(toggle.value);

      await user.type(input, "optional");
      expect(input.value, "optional");

      await user.click(toggle);
      expect(!toggle.value);

      await user.click(result.getByTestId("sub"));

      const value = form()!;
      expect(value.optional).toBeUndefined();
    });
  });
});
