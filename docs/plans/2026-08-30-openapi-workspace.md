# OpenAPI Workspace Refresh Implementation Plan

> **REQUIRED SUB-SKILL:** Use the executing-plans skill to implement this plan task-by-task.

**Goal:** Replace the unframed OpenAPI document with a focused, searchable, responsive RapiDoc explorer while preserving generated specifications, request execution, authentication, routes, and backend behavior.

**Architecture:** Keep RapiDoc as the only OpenAPI renderer and switch it to native focused navigation. A query-owned SolidJS workspace supplies loading/error/refresh states and metadata; small pure helpers handle operation counting, presentation-only tag collapsing, token validation, and request headers. Mobile exposes the same RapiDoc navigation as an overlay rather than creating a second endpoint index.

**Tech Stack:** SolidJS, TanStack Solid Query, Kobalte/Tailwind primitives, RapiDoc 9.3.8, Vitest, Solid Testing Library.

**Execution note:** The user explicitly approved continuing in the existing `feat/admin-ui-refresh` checkout rather than creating a worktree.

---

### Task 1: Add deterministic OpenAPI presentation and authentication helpers

**Files:**
- Create: `crates/assets/js/admin/src/components/openapi/openapi.ts`
- Create: `crates/assets/js/admin/tests/openapi-workspace.test.ts`

**Step 1: Write failing metadata and operation-count tests**

Cover only real OpenAPI HTTP operation keys. Ignore path-level `parameters`, `summary`, `$ref`, and extension keys.

```ts
const spec = {
  info: { title: "TrailBase", version: "0.33.5" },
  tags: [{ name: "admin" }],
  paths: {
    "/api/items": {
      parameters: [],
      get: { tags: ["admin"] },
      post: { tags: ["admin"] },
    },
    "/api/items/{id}": {
      $ref: "#/components/pathItems/item",
      delete: { tags: ["admin"] },
    },
  },
};

expect(openApiMetadata(spec)).toEqual({
  title: "TrailBase",
  version: "0.33.5",
  operationCount: 3,
});
```

Also test safe fallbacks for missing or malformed `info` and `paths`.

**Step 2: Run the focused test and verify failure**

Run:

```bash
pnpm --dir crates/assets/js/admin test -- tests/openapi-workspace.test.ts
```

Expected: FAIL because `@/components/openapi/openapi` does not exist.

**Step 3: Implement the smallest typed helpers**

Use a deliberately small local document shape rather than adding an OpenAPI typing dependency.

```ts
import type { Tokens } from "trailbase";

export type OpenApiDocument = {
  info?: { title?: unknown; version?: unknown };
  tags?: Array<Record<string, unknown> & { name?: unknown }>;
  paths?: Record<string, Record<string, unknown>>;
  [key: string]: unknown;
};

const operationMethods = new Set([
  "get",
  "put",
  "post",
  "delete",
  "patch",
  "options",
  "head",
  "trace",
]);

export function openApiMetadata(spec: OpenApiDocument) {
  let operationCount = 0;
  for (const path of Object.values(spec.paths ?? {})) {
    operationCount += Object.keys(path).filter((key) =>
      operationMethods.has(key.toLowerCase()),
    ).length;
  }

  return {
    title:
      typeof spec.info?.title === "string" ? spec.info.title : "OpenAPI",
    version:
      typeof spec.info?.version === "string" ? spec.info.version : undefined,
    operationCount,
  };
}
```

**Step 4: Add failing non-mutating collapsed-tag tests**

Assert that `withCollapsedOpenApiTags(spec)`:

- returns a new top-level object and new `tags` array
- adds `"x-tag-expanded": false` to each tag
- preserves every existing tag field
- does not mutate `spec` or its tag objects
- handles a missing `tags` array without throwing

**Step 5: Implement presentation cloning**

```ts
export function withCollapsedOpenApiTags(
  spec: OpenApiDocument,
): OpenApiDocument {
  if (!Array.isArray(spec.tags)) return spec;

  return {
    ...spec,
    tags: spec.tags.map((tag) => ({
      ...tag,
      "x-tag-expanded": false,
    })),
  };
}
```

Do not deep-clone or rewrite paths, operations, schemas, servers, or security definitions.

**Step 6: Add failing token validation and header tests**

Cover:

- empty input returns no override and no error
- valid base64 JSON returns the exact three token strings
- malformed base64, malformed JSON, non-object input, missing fields, null fields, and non-string fields are rejected with one generic validation result
- `applyRequestTokens()` sets exactly `Authorization`, `Refresh-Token`, and `CSRF-Token`
- setting headers replaces pre-existing values rather than appending duplicates
- no token value appears in an error string

Use a real `Request` and `Headers` where possible.

**Step 7: Implement token parsing and header application**

Keep validation explicit at the trust boundary.

```ts
export type RequestTokens = {
  auth_token: string;
  refresh_token: string;
  csrf_token: string;
};

export function parseImpersonationTokens(value: string): RequestTokens | null {
  if (!value.trim()) return null;

  try {
    const parsed: unknown = JSON.parse(atob(value.trim()));
    if (
      typeof parsed === "object" &&
      parsed !== null &&
      "auth_token" in parsed &&
      "refresh_token" in parsed &&
      "csrf_token" in parsed &&
      typeof parsed.auth_token === "string" &&
      typeof parsed.refresh_token === "string" &&
      typeof parsed.csrf_token === "string"
    ) {
      return parsed as RequestTokens;
    }
  } catch {
    // Return the same generic result for every invalid representation.
  }

  throw new Error("Invalid login tokens");
}

export function usableRequestTokens(
  tokens: Tokens | null | undefined,
): RequestTokens | null {
  return tokens &&
    typeof tokens.auth_token === "string" &&
    typeof tokens.refresh_token === "string" &&
    typeof tokens.csrf_token === "string"
    ? (tokens as RequestTokens)
    : null;
}

export function applyRequestTokens(request: Request, tokens: RequestTokens) {
  request.headers.set("Authorization", `Bearer ${tokens.auth_token}`);
  request.headers.set("Refresh-Token", tokens.refresh_token);
  request.headers.set("CSRF-Token", tokens.csrf_token);
}
```

The implementation may use a non-throwing discriminated result if it keeps the UI simpler, but tests must preserve the empty/valid/invalid distinction and confidentiality.

**Step 8: Run focused tests**

Run:

```bash
pnpm --dir crates/assets/js/admin test -- tests/openapi-workspace.test.ts
```

Expected: PASS.

**Step 9: Commit**

```bash
git add crates/assets/js/admin/src/components/openapi/openapi.ts \
  crates/assets/js/admin/tests/openapi-workspace.test.ts
git commit -m "feat(admin): add openapi workspace helpers"
```

---

### Task 2: Build the query-owned OpenAPI workspace shell

**Files:**
- Modify: `crates/assets/js/admin/src/components/openapi/OpenApiPage.tsx`
- Create: `crates/assets/js/admin/tests/openapi-workspace-ui.test.tsx`

**Step 1: Create a minimal RapiDoc test double**

Mock the side-effect `rapidoc` import and expose a test element with spies for `loadSpec`, `addEventListener`, `removeEventListener`, and `requestUpdate`. Mock `adminFetch`, `$tokens`, `$user`, `createTheme`, and TanStack `useQuery` using reactive getters, following the typed pattern in `tests/logs-workspace-ui.test.tsx`.

Do not replace the entire page with shallow string assertions. Render the real Solid component.

**Step 2: Write failing initial loading, success, and initial-error tests**

Verify:

- initial loading displays an accessible OpenAPI loading status
- success renders `OpenAPI Explorer`, server, version, and pluralized operation count
- success renders the current-session identity using email, then username, then `Admin session`
- first-load failure renders `Unable to load the API specification` and Retry
- raw error messages are absent
- Retry calls the query's `refetch`

**Step 3: Run the focused UI test and verify failure**

Run:

```bash
pnpm --dir crates/assets/js/admin test -- tests/openapi-workspace-ui.test.tsx
```

Expected: FAIL because the current page has no query-owned shell.

**Step 4: Replace imperative fetch with one query**

Keep the existing endpoint and native response parser:

```ts
const specQuery = useQuery(() => ({
  queryKey: ["openapi"],
  queryFn: async () => {
    const response = await adminFetch("/openapi.json");
    return (await response.json()) as OpenApiDocument;
  },
}));
```

Use guarded accessors if Solid Query can throw while error state is active, matching the defensive pattern already used by Logs.

**Step 5: Build the header and first-load states**

Reuse `Header`, `Button`, `Badge`, `Callout`, and existing semantic theme classes. The header should include:

- title and concise description
- version badge when available
- `N operations`
- server URL
- current-session identity
- Refresh button with visible pending text/state

Initial loading and initial error replace the explorer. Do not add a page-level card around RapiDoc.

**Step 6: Add failing authentication-disclosure tests**

Use native `<details>`/`<summary>` rather than adding another primitive. Verify:

- `Advanced authentication` starts closed
- the password input is absent from normal scan order until opened
- its label and description explain Accounts token origin
- valid input reports `Using impersonation tokens` without displaying token values
- invalid input shows `Invalid login tokens` and `aria-invalid="true"`
- clearing input restores `Using current admin session`
- the value is not persisted

**Step 7: Implement local authentication state**

Validate on input/change with the Task 1 helper. Store only the field value and validated token object in component-local signals. Never write impersonation tokens to local storage, query state, URL state, toast text, or console output.

**Step 8: Add failing retained-data refresh-error tests**

Seed existing query data and then expose an error with `isFetching=false`. Verify the explorer remains mounted and an inline generic refresh warning with Retry appears near the header.

**Step 9: Implement retained-data error behavior**

Only show the full-page error when no data exists. When data exists, preserve the explorer and show a compact warning. Refresh must not clear the token field.

**Step 10: Run focused tests**

Run:

```bash
pnpm --dir crates/assets/js/admin test -- tests/openapi-workspace-ui.test.tsx
```

Expected: PASS.

**Step 11: Commit**

```bash
git add crates/assets/js/admin/src/components/openapi/OpenApiPage.tsx \
  crates/assets/js/admin/tests/openapi-workspace-ui.test.tsx
git commit -m "feat(admin): frame openapi explorer workspace"
```

---

### Task 3: Integrate focused RapiDoc lifecycle, themes, and mobile navigation

**Files:**
- Modify: `crates/assets/js/admin/src/components/openapi/OpenApiPage.tsx`
- Modify: `crates/assets/js/admin/src/index.css`
- Modify: `crates/assets/js/admin/tests/openapi-workspace-ui.test.tsx`

**Step 1: Write failing exact-configuration tests**

Assert the rendered `rapi-doc` has the intended supported RapiDoc 9.3.8 attributes:

```text
render-style="focused"
layout="row"
schema-style="table"
show-header="false"
show-side-nav="true"
allow-search="true"
nav-item-spacing="compact"
show-method-in-nav-bar="as-colored-block"
use-path-in-nav-bar="true"
allow-try="true"
persist-auth="false"
allow-authentication="false"
allow-server-selection="false"
load-fonts="false"
```

Do not assert unrelated internal shadow-DOM class names.

**Step 2: Write failing load/lifecycle tests**

Verify:

- `loadSpec()` receives the collapsed-tag presentation clone
- the fetched source object remains unchanged
- an unchanged query result is not loaded again on theme changes or local token edits
- a new successful specification result loads once
- development sets `server-url` and `default-api-server` to `http://localhost:4000`
- `spec-loaded` and `before-try` listeners are attached once and removed on cleanup

**Step 3: Implement one-time listener lifecycle and reactive loading**

Use `onMount` for listener registration, `onCleanup` for removal, and a `createEffect` that depends only on query data for `loadSpec(withCollapsedOpenApiTags(spec))`.

Keep the existing `spec-loaded` correction for RapiDoc's `api-info` negative margin only if it is still required in focused mode. Guard all shadow-root access.

**Step 4: Write failing request-authentication tests**

Capture the registered `before-try` callback and call it with a real request. Verify:

- current-session tokens are used by default
- validated impersonation tokens take precedence
- invalid override input never injects malformed or partial values
- clearing the override restores current-session headers
- repeated callback invocation on a request replaces headers and never duplicates values

**Step 5: Implement request authentication**

At event time, read the latest validated override signal; otherwise validate `$tokens.get()` with `usableRequestTokens()`. Apply headers only when one complete token set is available.

Use a narrow local event type instead of `any`:

```ts
type BeforeTryEvent = CustomEvent<{
  request: Request;
}>;
```

**Step 6: Write failing theme tests**

Verify light/dark updates change RapiDoc's `theme`, `bg-color`, `text-color`, `nav-bg-color`, `nav-text-color`, and primary color attributes without invoking `loadSpec()` again. Use supported hex palette values because RapiDoc validates color attributes as hex.

**Step 7: Implement semantic matching palettes**

Use one small light/dark palette map colocated with the RapiDoc configuration. Match the existing admin semantic colors closely; do not add a general theme abstraction for one integration.

**Step 8: Write failing mobile endpoint-browser tests**

Mock the existing responsive signal/predicate and verify:

- desktop navigation is visible without an extra button
- mobile renders `Browse endpoints`
- the button has accurate `aria-expanded`
- clicking toggles only an `openapi-nav-open` state/class on the same RapiDoc element
- documentation remains mounted while the navigation opens/closes

**Step 9: Implement responsive navigation using the native RapiDoc part**

Add narrowly scoped CSS in `src/index.css` using `.openapi-explorer::part(section-navbar)`. At the mobile breakpoint:

- hide the navbar by default
- display it as an absolute/fixed overlay below the workspace header when `.openapi-nav-open` is present
- cap width to the viewport
- keep main documentation full width
- preserve internal navbar scrolling
- ensure the application body width never exceeds the viewport

Do not inspect or depend on RapiDoc's private `.nav-bar-path` DOM classes.

**Step 10: Run focused and neighboring tests**

Run:

```bash
pnpm --dir crates/assets/js/admin test -- \
  tests/openapi-workspace.test.ts \
  tests/openapi-workspace-ui.test.tsx \
  tests/sidebar.test.tsx
```

Expected: PASS.

**Step 11: Commit**

```bash
git add crates/assets/js/admin/src/components/openapi/OpenApiPage.tsx \
  crates/assets/js/admin/src/index.css \
  crates/assets/js/admin/tests/openapi-workspace-ui.test.tsx
git commit -m "feat(admin): focus openapi endpoint exploration"
```

---

### Task 4: Exact-spec review, quality review, and browser acceptance

**Files:**
- Modify only if a failing regression test demonstrates a defect.

**Step 1: Run the complete automated suite**

```bash
pnpm --dir crates/assets/js/admin check:format
pnpm --dir crates/assets/js/admin check
pnpm --dir crates/assets/js/admin build
git diff --check
git status --short
```

Expected:

- formatting passes
- TypeScript passes
- ESLint has no new warnings; the four pre-existing `OpenApiPage.tsx` warnings should be removed by the rewrite rather than carried forward
- all existing and new Vitest tests pass
- production build passes
- no whitespace errors
- clean working tree after commits

**Step 2: Run an exact-spec review**

Review the complete OpenAPI commit range against:

- `docs/plans/2026-08-30-openapi-workspace-design.md`
- this implementation plan

Confirm route/API preservation, native RapiDoc rendering, focused exploration, collapsed groups, authentication semantics, retained refresh data, responsive navigation, accessibility, and all non-goals.

**Step 3: Run a code-quality/security/accessibility review**

Inspect:

- token confidentiality and validation
- listener cleanup and duplicate-header prevention
- query/error reactivity
- specification immutability
- theme updates without reload
- CSS-part scoping and mobile overflow
- custom-element typing
- tests that exercise behavior rather than implementation trivia
- no new dependency or backend changes

For every blocker, first add a failing regression test, then apply the minimum fix and rerun focused checks.

**Step 4: Browser acceptance at desktop `1440×900`**

Validate light and dark themes:

- header hierarchy and metadata
- current-session identity
- search and initially collapsed endpoint groups
- group expansion and endpoint selection
- one-operation focused content
- schemas, examples, response bodies, and cURL
- current-session Try request and exact headers
- valid impersonation override without credential disclosure
- invalid and cleared override behavior
- specification refresh while preserving visible content
- initial and retained-data errors using controlled network failure
- keyboard focus order and visible focus
- contained overflow
- console and network output

Capture final screenshots.

**Step 5: Browser acceptance at mobile `390×844`**

Validate:

- compact stacked header
- mobile application header/sidebar shell does not overlap the workspace
- endpoint navigation starts closed
- `Browse endpoints` opens the same native RapiDoc navigation as an overlay
- search, group expansion, and endpoint selection remain usable
- closing navigation returns full width to documentation
- Try, authentication disclosure, schemas, and response bodies remain accessible
- document/body width remains `390px`

Restore desktop dimensions after the check.

**Step 6: Final verification and commit only if browser fixes were needed**

Repeat Step 1. If browser-driven changes exist:

```bash
git add <changed-files>
git commit -m "fix(admin): polish openapi workspace"
```

**Step 7: Push only after user confirmation**

Do not push automatically. Report final commits, test count, reviews, browser evidence, screenshots, and any unrelated warnings.
