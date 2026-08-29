import { fireEvent, render } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";

import { FilterBar } from "@/components/FilterBar";

describe("FilterBar", () => {
  it("submits the current input value", async () => {
    const onSubmit = vi.fn();
    const result = render(() => (
      <FilterBar initial="id > 2" onSubmit={onSubmit} />
    ));
    const input = result.getByRole("textbox", { name: "Filter rows" });

    await fireEvent.input(input, { target: { value: "name = 'Ada'" } });
    await fireEvent.click(result.getByRole("button", { name: "Apply filter" }));

    expect(onSubmit).toHaveBeenCalledWith("name = 'Ada'");
  });

  it("clears the visible input and submitted filter", async () => {
    const onSubmit = vi.fn();
    const result = render(() => (
      <FilterBar initial="id > 2" onSubmit={onSubmit} />
    ));
    const input = result.getByRole("textbox", {
      name: "Filter rows",
    }) as HTMLInputElement;

    await fireEvent.click(result.getByRole("button", { name: "Clear filter" }));

    expect(input.value).toBe("");
    expect(onSubmit).toHaveBeenCalledWith("");
    expect(result.queryByRole("button", { name: "Clear filter" })).toBeNull();
  });

  it("does not show clear when the filter is empty", () => {
    const result = render(() => <FilterBar onSubmit={() => undefined} />);

    expect(result.queryByRole("button", { name: "Clear filter" })).toBeNull();
  });
});
