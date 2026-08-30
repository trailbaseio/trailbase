import type { LogJson } from "@bindings/LogJson";

export type LogStatusTone = "success" | "muted" | "warning" | "destructive";

export function formatLogTimestamp(created: number) {
  const iso = new Date(created * 1000).toISOString();
  return { date: iso.slice(0, 10), time: iso.slice(11, 23), iso };
}

export function formatLogLatency(latencyMs: number): string {
  if (latencyMs < 1000) {
    return `${latencyMs < 10 ? latencyMs.toFixed(2) : latencyMs.toFixed(1)} ms`;
  }
  return `${(latencyMs / 1000).toFixed(2)} s`;
}

export function logStatusTone(status: number): LogStatusTone {
  if (status >= 200 && status < 300) return "success";
  if (status >= 300 && status < 400) return "muted";
  if (status >= 400 && status < 500) return "warning";
  if (status >= 500) return "destructive";
  return "muted";
}

export function logClientLabel(log: LogJson): string {
  const city = log.client_geoip_city?.name;
  const country = log.client_geoip_city?.country_code ?? log.client_geoip_cc;
  if (city && country) return `${city}, ${country}`;
  return city ?? country ?? log.client_ip;
}
