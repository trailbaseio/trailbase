import { fireEvent, render } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";

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
  it("shows complete request details and copies exact values without leaking them", async () => {
    const onClose = vi.fn();
    const result = render(() => (
      <LogDetailsSheet log={log} onClose={onClose} />
    ));

    expect(
      result.getByRole("heading", { name: "POST /api/widgets" }),
    ).toBeInTheDocument();
    expect(result.getByText("Request details")).toBeInTheDocument();
    for (const heading of [
      "Request",
      "Timing",
      "Client and location",
      "Identity",
      "Metadata",
    ]) {
      expect(
        result.getByRole("heading", { name: heading }),
      ).toBeInTheDocument();
    }
    for (const value of [
      "42",
      "2023-11-14",
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
      expect(result.getByText(value)).toBeInTheDocument();
    }

    await fireEvent.click(result.getByRole("button", { name: "Copy url" }));
    expect(copyToClipboard).toHaveBeenCalledWith(
      "/api/widgets",
      false,
      "url copied",
    );
    expect(result.container.textContent).not.toContain("Copied: /api/widgets");

    await fireEvent.keyDown(result.getByRole("button", { name: "Close" }), {
      key: "Enter",
    });
    expect(onClose).toHaveBeenCalled();
  });

  it("uses an em dash for absent optional values and closes through onOpenChange", async () => {
    const onClose = vi.fn();
    const result = render(() => (
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
    expect(result.getAllByText("—").length).toBeGreaterThanOrEqual(5);
    await fireEvent.click(result.getByRole("button", { name: "Close" }));
    expect(onClose).toHaveBeenCalled();
  });
});
