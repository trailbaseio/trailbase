import { beforeEach, describe, expect, it, vi } from "vitest";

const adminFetch = vi.hoisted(() => vi.fn());
vi.mock("@/lib/fetch", () => ({ adminFetch }));

import { fetchLogs, fetchStats } from "@/lib/api/logs";

const response = (body: string): Response =>
  new Response(body, { headers: { "content-type": "application/json" } });

describe("logs API JSON parsing", () => {
  beforeEach(() => adminFetch.mockReset());

  it("preserves large integer IDs, totals, and rate timestamps", async () => {
    adminFetch
      .mockResolvedValueOnce(
        response(
          '{"total_row_count":9007199254740993,"cursor":null,"entries":[{"id":9007199254740993,"created":1700000000,"status":200,"method":"GET","url":"/api/items","latency_ms":1.5,"client_ip":"127.0.0.1","client_geoip_cc":null,"client_geoip_city":null,"referer":"","user_agent":"test","user_id":null}]}',
        ),
      )
      .mockResolvedValueOnce(
        response('{"rates":[[9007199254740993,2.5]],"country_codes":null}'),
      );

    const logs = await fetchLogs(20, 0);
    const stats = await fetchStats();

    expect(logs.total_row_count).toBe(9007199254740993n);
    expect(logs.entries[0].id).toBe(9007199254740993n);
    expect(logs.entries[0].url).toBe("/api/items");
    expect(logs.entries[0].latency_ms).toBe(1.5);
    expect(stats.rates[0]).toEqual([9007199254740993n, 2.5]);
  });
});
