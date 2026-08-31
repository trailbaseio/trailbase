import { beforeEach, describe, expect, it, vi } from "vitest";

import { Config } from "@proto/config";
import { UpdateConfigRequest } from "@proto/config_api";

const mocks = vi.hoisted(() => ({
  adminFetch: vi.fn(),
  showToast: vi.fn(),
}));

vi.mock("@/lib/fetch", () => ({ adminFetch: mocks.adminFetch }));
vi.mock("@/components/ui/toast", () => ({ showToast: mocks.showToast }));

import { setConfig } from "@/lib/api/config";

function client(
  hash?: string,
  invalidateQueries = vi.fn().mockResolvedValue(undefined),
) {
  return {
    getQueryData: vi.fn(() => (hash ? { hash } : undefined)),
    invalidateQueries,
  } as any;
}

const config = () =>
  Config.fromPartial({ server: { applicationName: "Secret application" } });

beforeEach(() => {
  mocks.adminFetch.mockReset();
  mocks.adminFetch.mockResolvedValue({});
  mocks.showToast.mockReset();
});

describe("config API", () => {
  it("rejects missing hashes in throw mode without issuing a POST", async () => {
    await expect(
      setConfig({ client: client(), config: config(), throw: true }),
    ).rejects.toThrow("Missing config hash");
    expect(mocks.adminFetch).not.toHaveBeenCalled();
  });

  it("handles missing hashes generically in nonthrow mode without leaking data", async () => {
    const debug = vi.spyOn(console, "debug").mockImplementation(() => {});
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    await setConfig({ client: client(), config: config(), throw: false });
    expect(mocks.adminFetch).not.toHaveBeenCalled();
    expect(mocks.showToast).toHaveBeenCalledWith({
      title: "Config update failed",
      description: "Please refresh and try again.",
      variant: "error",
    });
    expect(JSON.stringify(mocks.showToast.mock.calls)).not.toContain(
      "Secret application",
    );
    expect(debug).not.toHaveBeenCalled();
    expect(error).not.toHaveBeenCalled();
    debug.mockRestore();
    error.mockRestore();
  });

  it("posts exact protobuf data and awaits invalidation", async () => {
    const debug = vi.spyOn(console, "debug").mockImplementation(() => {});
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    let finishInvalidation!: () => void;
    const invalidateQueries = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          finishInvalidation = resolve;
        }),
    );
    const pending = setConfig({
      client: client("hash-123", invalidateQueries),
      config: config(),
      throw: true,
    });
    let settled = false;
    void pending.then(() => {
      settled = true;
    });
    await vi.waitFor(() => expect(mocks.adminFetch).toHaveBeenCalledOnce());
    const [url, options] = mocks.adminFetch.mock.calls[0];
    expect(url).toBe("/config");
    expect(options.method).toBe("POST");
    const request = UpdateConfigRequest.decode(options.body);
    expect(request.hash).toBe("hash-123");
    expect(request.config?.server?.applicationName).toBe("Secret application");
    await vi.waitFor(() =>
      expect(invalidateQueries).toHaveBeenCalledWith({
        queryKey: ["admin", "proto_config"],
      }),
    );
    expect(settled).toBe(false);

    finishInvalidation();
    await pending;
    expect(settled).toBe(true);
    expect(debug).not.toHaveBeenCalled();
    expect(error).not.toHaveBeenCalled();
    debug.mockRestore();
    error.mockRestore();
  });
});
