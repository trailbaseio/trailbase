import { describe, expect, test } from "vitest";
import type { LogJson } from "@bindings/LogJson";
import {
  formatLogLatency,
  formatLogTimestamp,
  logClientLabel,
  logStatusTone,
} from "@/components/logs/logs";

const log = (overrides: Partial<LogJson> = {}): LogJson => ({
  id: 1n,
  created: 0,
  status: 200,
  method: "GET",
  url: "/",
  latency_ms: 1,
  client_ip: "192.0.2.1",
  client_geoip_cc: null,
  client_geoip_city: null,
  referer: "",
  user_agent: "",
  user_id: null,
  ...overrides,
});

describe("log presentation helpers", () => {
  test("formats timestamps in UTC", () => {
    expect(formatLogTimestamp(1_700_000_000)).toEqual({
      date: "2023-11-14",
      time: "22:13:20.000",
      iso: "2023-11-14T22:13:20.000Z",
    });
  });

  test("formats latency at readable boundaries", () => {
    expect(formatLogLatency(0)).toBe("0.00 ms");
    expect(formatLogLatency(0.882209)).toBe("0.88 ms");
    expect(formatLogLatency(29.313958)).toBe("29.3 ms");
    expect(formatLogLatency(999.9)).toBe("999.9 ms");
    expect(formatLogLatency(1000)).toBe("1.00 s");
    expect(formatLogLatency(1250)).toBe("1.25 s");
  });

  test("maps HTTP status classes to tones", () => {
    expect(logStatusTone(199)).toBe("muted");
    expect(logStatusTone(204)).toBe("success");
    expect(logStatusTone(302)).toBe("muted");
    expect(logStatusTone(404)).toBe("warning");
    expect(logStatusTone(503)).toBe("destructive");
  });

  test("labels geoip clients and falls back to IP", () => {
    expect(
      logClientLabel(
        log({ client_geoip_city: { name: "Paris", country_code: "FR" } }),
      ),
    ).toBe("Paris, FR");
    expect(logClientLabel(log({ client_geoip_cc: "DE" }))).toBe("DE");
    expect(logClientLabel(log())).toBe("192.0.2.1");
    expect(
      logClientLabel(
        log({ client_geoip_city: { name: null, country_code: null } }),
      ),
    ).toBe("192.0.2.1");
  });
});
