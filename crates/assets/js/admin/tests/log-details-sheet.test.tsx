import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { LogJson } from "@bindings/LogJson";
import { LogDetailsSheet } from "@/components/logs/LogDetailsSheet";
import { copyToClipboard } from "@/lib/utils";

vi.mock("@/lib/utils", async () => {
  const actual =
    await vi.importActual<typeof import("@/lib/utils")>("@/lib/utils");
  return { ...actual, copyToClipboard: vi.fn().mockResolvedValue(undefined) };
});

const log: LogJson = {
  id: 42n,
  created: 1_700_000_000,
  status: 201,
  method: "POST",
  url: "/api/widgets",
  latency_ms: 29.313958,
  client_ip: "192.0.2.1",
  client_geoip_cc: "FR",
  client_geoip_city: { name: "Paris", country_code: "FR" },
  referer: "https://example.test",
  user_agent: "TestAgent/1.0",
  user_id: "user-7",
};

describe("LogDetailsSheet", () => {
  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("shows complete request details and copies exact values without leaking them", async () => {
    const onClose = vi.fn();
    render(() => <LogDetailsSheet log={log} onClose={onClose} />);

    expect(
      screen.getByRole("heading", { name: "POST /api/widgets" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Request details")).toBeInTheDocument();
    for (const heading of [
      "Request",
      "Timing",
      "Client and location",
      "Identity",
      "Metadata",
    ]) {
      expect(
        screen.getByRole("heading", { name: heading }),
      ).toBeInTheDocument();
    }
    for (const value of [
      "42",
      "2023-11-14T22:13:20.000Z",
      "201",
      "POST",
      "/api/widgets",
      "29.3 ms",
      "192.0.2.1",
      "FR",
      "Paris, FR",
      "https://example.test",
      "TestAgent/1.0",
      "user-7",
    ]) {
      expect(screen.getByText(value)).toBeInTheDocument();
    }

    await fireEvent.click(screen.getByRole("button", { name: "Copy url" }));
    expect(copyToClipboard).toHaveBeenCalledWith(
      "/api/widgets",
      false,
      "url copied",
    );
    expect(document.body.textContent).not.toContain("Copied: /api/widgets");

    await fireEvent.click(screen.getByRole("button", { name: "Dismiss" }));
    expect(onClose).toHaveBeenCalled();
  });

  it("uses an em dash for absent optional values and closes through onOpenChange", async () => {
    const onClose = vi.fn();
    render(() => (
      <LogDetailsSheet
        log={{
          ...log,
          client_geoip_cc: null,
          client_geoip_city: null,
          referer: "",
          user_agent: "",
          user_id: null,
        }}
        onClose={onClose}
      />
    ));
    expect(screen.getAllByText("—").length).toBeGreaterThanOrEqual(5);
    await fireEvent.click(screen.getByRole("button", { name: "Dismiss" }));
    expect(onClose).toHaveBeenCalled();
  });
});
