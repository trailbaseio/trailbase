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
  buildOptionalIntegerFormField,
  buildOptionalSmallIntegerFormField,
  type FieldApiT,
} from "@/components/FormFields";

interface MyForm {
  required: string;
  optional: string | undefined;
  nullable: string | null;
  optionalNullable: string | null | undefined;
  bigint: bigint | undefined;
  smallInteger: number | undefined;
}

function Form(props: {
  name: DeepKeys<MyForm>;
  setForm: Setter<MyForm | undefined>;
  defaultValue?: MyForm;
  field: (field: () => FieldApiT<any>) => JSX.Element;
  resetValue?: MyForm;
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
      {props.resetValue && (
        <button type="button" onClick={() => form.reset(props.resetValue)}>
          Reset test form
        </button>
      )}
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

describe("number form fields", () => {
  test("reset updates the controlled small integer input", async () => {
    const user = userEvent.setup();
    const [, setSubmitted] = createSignal<MyForm | undefined>();
    const defaults = {
      required: "default",
      nullable: null,
      smallInteger: 3600,
    } as MyForm;
    const dom = render(() => (
      <Form
        name="smallInteger"
        setForm={setSubmitted}
        defaultValue={defaults}
        resetValue={{ ...defaults, smallInteger: 7200 }}
        field={buildOptionalSmallIntegerFormField({
          label: () => "Small integer",
        })}
      />
    ));

    const input = dom.getByTestId("input") as HTMLInputElement;
    expect(input.value).toBe("3600");
    await user.click(dom.getByRole("button", { name: "Reset test form" }));
    expect(input.value).toBe("7200");
  });

  test("keeps bigint exact and reset updates the controlled input", async () => {
    const user = userEvent.setup();
    const [submitted, setSubmitted] = createSignal<MyForm | undefined>();
    const defaults = {
      required: "default",
      nullable: null,
      bigint: 3600n,
    } as MyForm;
    const resetValue = { ...defaults, bigint: 7200n };
    const dom = render(() => (
      <Form
        name="bigint"
        setForm={setSubmitted}
        defaultValue={defaults}
        resetValue={resetValue}
        field={buildOptionalIntegerFormField({ label: () => "Big integer" })}
      />
    ));

    const input = dom.getByTestId("input") as HTMLInputElement;
    expect(input.value).toBe("3600");
    await user.clear(input);
    await user.type(input, "9007199254740993");
    await user.tab();
    await user.click(dom.getByTestId("sub"));
    expect(submitted()!.bigint).toBe(9007199254740993n);

    await user.click(dom.getByRole("button", { name: "Reset test form" }));
    expect(input.value).toBe("7200");
    await user.click(dom.getByTestId("sub"));
    expect(submitted()!.bigint).toBe(7200n);
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
