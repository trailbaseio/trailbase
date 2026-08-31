import { render, screen } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
import { SettingsFormActions } from "@/components/settings/SettingsFormActions";

describe("settings workspace", () => {
  it("renders shared actions only when dirty", () => {
    const onReset = vi.fn();
    const { container } = render(() => (
      <SettingsFormActions dirty={false} canSubmit={true} isSubmitting={false} onReset={onReset} />
    ));
    expect(container.textContent).not.toContain("Save changes");
    expect(screen.queryByRole("button", { name: "Reset" })).toBeNull();
  });

  it("exposes reset and save actions when dirty and disables while submitting", () => {
    render(() => (
      <SettingsFormActions dirty={true} canSubmit={false} isSubmitting={true} onReset={vi.fn()} />
    ));
    expect(screen.getByRole("button", { name: "Reset" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save changes" })).toBeDisabled();
  });
});
