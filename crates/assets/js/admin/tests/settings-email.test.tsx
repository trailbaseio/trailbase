/* eslint-disable @typescript-eslint/no-explicit-any */
import { describe, expect, test, vi } from "vitest";
import { render, screen } from "@solidjs/testing-library";

const config = { config: {} };
vi.mock("@tanstack/solid-query", () => ({ useQueryClient: () => ({}) }));
vi.mock("@nanostores/solid", () => ({
  useStore: () => () => ({ email: "test@example.com" }),
}));
vi.mock("@/lib/api/config", () => ({
  createConfigQuery: () => ({ data: config, isLoading: false, isError: false }),
  setConfig: vi.fn(),
}));
vi.mock("@/lib/client", () => ({
  $user: { get: () => ({ email: "test@example.com" }) },
}));
vi.mock("@/lib/fetch", () => ({ adminFetch: vi.fn() }));

import { EmailSettings } from "@/components/settings/EmailSettings";

describe("Email settings", () => {
  test("renders an editable empty form when email config is absent", () => {
    render(() => <EmailSettings setDirty={vi.fn()} postSubmit={vi.fn()} />);
    expect(screen.getByRole("heading", { name: "SMTP" })).toBeInTheDocument();
    expect(screen.getByLabelText("Host")).toBeInTheDocument();
    expect(screen.getByLabelText("Password")).toHaveAttribute(
      "type",
      "password",
    );
  });

  test("does not expose a secret in the rendered form", () => {
    const secret = "sentinel-email-secret";
    render(() => <EmailSettings setDirty={vi.fn()} postSubmit={vi.fn()} />);
    expect(document.body.textContent).not.toContain(secret);
    expect(screen.getAllByLabelText("Password")[0]).toHaveAttribute(
      "type",
      "password",
    );
  });
});
