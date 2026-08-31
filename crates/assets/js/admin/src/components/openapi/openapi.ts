import type { Tokens } from "trailbase";

export type OpenApiDocument = {
  info?: unknown;
  paths?: unknown;
  tags?: unknown;
  [key: string]: unknown;
};

export type OpenApiMetadata = {
  title: string;
  version?: string;
  operationCount: number;
};

export type RequestTokens = {
  auth_token: string;
  refresh_token: string;
  csrf_token: string;
};

const OPERATIONS = new Set([
  "get",
  "put",
  "post",
  "delete",
  "patch",
  "options",
  "head",
  "trace",
]);

export function openApiMetadata(spec: unknown): OpenApiMetadata {
  const document = isRecord(spec) ? spec : {};
  const info = isRecord(document.info) ? document.info : {};
  const paths = isRecord(document.paths) ? document.paths : {};
  let operationCount = 0;

  for (const path of Object.values(paths)) {
    if (!isRecord(path)) continue;
    for (const key of Object.keys(path)) {
      if (OPERATIONS.has(key.toLowerCase())) operationCount++;
    }
  }

  return {
    title: typeof info.title === "string" ? info.title : "OpenAPI",
    ...(typeof info.version === "string" ? { version: info.version } : {}),
    operationCount,
  };
}

export function withCollapsedOpenApiTags(spec: unknown): OpenApiDocument {
  if (!isRecord(spec)) return {};

  const existing = Array.isArray(spec.tags)
    ? spec.tags.filter(
        (tag): tag is Record<string, unknown> =>
          isRecord(tag) && typeof tag.name === "string" && tag.name.length > 0,
      )
    : [];
  const names = new Set(existing.map((tag) => tag.name as string));
  const operationTags: string[] = [];
  const paths = isRecord(spec.paths) ? spec.paths : {};
  for (const path of Object.values(paths)) {
    if (!isRecord(path)) continue;
    for (const [method, operation] of Object.entries(path)) {
      if (!OPERATIONS.has(method.toLowerCase()) || !isRecord(operation))
        continue;
      if (!Array.isArray(operation.tags)) continue;
      for (const tag of operation.tags) {
        if (typeof tag === "string" && tag.length > 0 && !names.has(tag)) {
          names.add(tag);
          operationTags.push(tag);
        }
      }
    }
  }
  if (existing.length === 0 && operationTags.length === 0) return { ...spec };

  return {
    ...spec,
    tags: [
      ...existing.map((tag) => ({ ...tag, "x-tag-expanded": false })),
      ...operationTags.map((name) => ({ name, "x-tag-expanded": false })),
    ],
  };
}

export function parseImpersonationTokens(input: string): RequestTokens | null {
  const encoded = input.trim();
  if (encoded === "") return null;

  try {
    const decoded: unknown = JSON.parse(atob(encoded));
    const keys = isRecord(decoded) ? Object.keys(decoded) : [];
    const expectedKeys = ["auth_token", "refresh_token", "csrf_token"];
    if (
      !isRecord(decoded) ||
      keys.length !== expectedKeys.length ||
      expectedKeys.some((key) => !keys.includes(key)) ||
      typeof decoded.auth_token !== "string" ||
      typeof decoded.refresh_token !== "string" ||
      typeof decoded.csrf_token !== "string"
    ) {
      throw new Error();
    }
    return {
      auth_token: decoded.auth_token,
      refresh_token: decoded.refresh_token,
      csrf_token: decoded.csrf_token,
    };
  } catch {
    throw new Error("Invalid login tokens");
  }
}

export function usableRequestTokens(
  tokens: Tokens | null | undefined,
): RequestTokens | null {
  if (
    !tokens ||
    typeof tokens.auth_token !== "string" ||
    typeof tokens.refresh_token !== "string" ||
    typeof tokens.csrf_token !== "string"
  ) {
    return null;
  }
  return {
    auth_token: tokens.auth_token,
    refresh_token: tokens.refresh_token,
    csrf_token: tokens.csrf_token,
  };
}

export function resolveOpenApiServer(candidate: unknown, dev: boolean): string {
  const origin = window.location.origin;
  if (dev) return "http://localhost:4000";
  if (typeof candidate === "string") {
    try {
      const url = new URL(candidate, origin);
      if (
        (url.protocol === "http:" || url.protocol === "https:") &&
        url.origin === origin
      )
        return url.href.replace(/\/$/, "");
    } catch {
      /* invalid candidate */
    }
  }
  return origin;
}

export function requestHasCredentialOrigin(url: string, dev: boolean): boolean {
  try {
    const target = new URL(url, window.location.origin);
    const expected = dev ? "http://localhost:4000" : window.location.origin;
    return (
      (target.protocol === "http:" || target.protocol === "https:") &&
      target.origin === expected
    );
  } catch {
    return false;
  }
}

export function applyRequestTokens(
  request: { headers: Headers },
  tokens: RequestTokens,
): void {
  request.headers.set("Authorization", `Bearer ${tokens.auth_token}`);
  request.headers.set("Refresh-Token", tokens.refresh_token);
  request.headers.set("CSRF-Token", tokens.csrf_token);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
