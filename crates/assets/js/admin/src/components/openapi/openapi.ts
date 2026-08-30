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
  if (!Array.isArray(spec.tags)) return { ...spec };

  return {
    ...spec,
    tags: spec.tags.map((tag) =>
      isRecord(tag) ? { ...tag, "x-tag-expanded": false } : tag,
    ),
  };
}

export function parseImpersonationTokens(input: string): RequestTokens | null {
  if (input === "") return null;

  try {
    const decoded: unknown = JSON.parse(atob(input));
    if (
      !isRecord(decoded) ||
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

export function applyRequestTokens(
  request: Request,
  tokens: RequestTokens,
): void {
  request.headers.set("Authorization", `Bearer ${tokens.auth_token}`);
  request.headers.set("Refresh-Token", tokens.refresh_token);
  request.headers.set("CSRF-Token", tokens.csrf_token);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
