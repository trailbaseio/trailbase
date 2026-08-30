import { render, screen } from "@solidjs/testing-library";
import { describe, expect, it } from "vitest";
import { LogsInsights } from "@/components/logs/LogsInsights";

describe("LogsInsights", () => {
  it("exposes an accessible Activity disclosure", () => {
    render(() => <LogsInsights rates={[]} countryCodes={null} />);
    const button = screen.getByRole("button", { name: "Activity" });
    expect(button).toHaveAttribute("aria-expanded");
  });
});
