import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";

import { AccountToolbar } from "@/components/accounts/AccountsPage";

afterEach(cleanup);

describe("AccountToolbar", () => {
  it("switches between simple and advanced account filters", async () => {
    const onModeChange = vi.fn();
    render(() => (
      <AccountToolbar advanced={false} onModeChange={onModeChange}>
        <input aria-label="Search accounts" />
      </AccountToolbar>
    ));

    expect(
      screen.getByRole("button", { name: "Search accounts" }),
    ).toBeVisible();
    expect(
      screen.getByRole("textbox", { name: "Search accounts" }),
    ).toBeVisible();
    await fireEvent.click(
      screen.getByRole("button", { name: "Advanced account filter" }),
    );
    expect(onModeChange).toHaveBeenCalledWith(true);
  });

  it("supports switching back to simple search", async () => {
    const onModeChange = vi.fn();
    render(() => (
      <AccountToolbar advanced={true} onModeChange={onModeChange}>
        <input aria-label="Advanced account filter" />
      </AccountToolbar>
    ));
    await fireEvent.click(
      screen.getByRole("button", { name: "Search accounts" }),
    );
    expect(onModeChange).toHaveBeenCalledWith(false);
  });

  it("keeps the active mode and its apply/clear actions accessible", async () => {
    const onModeChange = vi.fn();
    const onSubmit = vi.fn();
    render(() => (
      <AccountToolbar advanced={false} onModeChange={onModeChange}>
        <form
          onSubmit={(event) => {
            event.preventDefault();
            onSubmit();
          }}
        >
          <input aria-label="Search accounts" />
          <button type="button" onClick={() => onSubmit("")}>
            Clear filter
          </button>
          <button type="submit">Apply filter</button>
        </form>
        <button
          aria-label="Refresh accounts"
          onClick={() => onSubmit("refresh")}
        >
          Refresh
        </button>
        <button onClick={() => onSubmit("add")}>Add account</button>
      </AccountToolbar>
    ));

    expect(
      screen.getByRole("textbox", { name: "Search accounts" }),
    ).toBeVisible();
    expect(screen.getByRole("button", { name: "Apply filter" })).toBeVisible();
    await fireEvent.click(screen.getByRole("button", { name: "Apply filter" }));
    await fireEvent.click(screen.getByRole("button", { name: "Clear filter" }));
    await fireEvent.click(
      screen.getByRole("button", { name: "Refresh accounts" }),
    );
    await fireEvent.click(screen.getByRole("button", { name: "Add account" }));
    expect(onSubmit).toHaveBeenNthCalledWith(1);
    expect(onSubmit).toHaveBeenNthCalledWith(2, "");
    expect(onSubmit).toHaveBeenNthCalledWith(3, "refresh");
    expect(onSubmit).toHaveBeenNthCalledWith(4, "add");
  });
});
