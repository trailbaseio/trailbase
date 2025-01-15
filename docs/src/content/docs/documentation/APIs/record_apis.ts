type ApiOptions = {
  name: string;
  suffix?: string;
  prefix?: string;
};

export const recordApiNamePlaceholder = "<record_api_name>";
export const recordApiIdPlaceholder = "<url-safe_b64_uuid_or_int>";

export function apiPath(opts: ApiOptions): string {
  const apiBase = "/api/records/v1";
  let suffix = opts.suffix ? `/${opts.suffix}` : "";
  return `${opts.prefix ?? ""}${apiBase}/${opts.name}${suffix}`;
}
