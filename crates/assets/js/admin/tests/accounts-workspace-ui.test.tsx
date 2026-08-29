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

    expect(screen.getByRole("button", { name: "Search accounts" })).toBeVisible();
    expect(screen.getByRole("textbox", { name: "Search accounts" })).toBeVisible();
    await fireEvent.click(screen.getByRole("button", { name: "Advanced account filter" }));
    expect(onModeChange).toHaveBeenCalledWith(true);
  });

  it("supports switching back to simple search", async () => {
    const onModeChange = vi.fn();
    render(() => (
      <AccountToolbar advanced={true} onModeChange={onModeChange}>
        <input aria-label="Advanced account filter" />
      </AccountToolbar>
    ));
    await fireEvent.click(screen.getByRole("button", { name: "Search accounts" }));
    expect(onModeChange).toHaveBeenCalledWith(false);
  });
});
