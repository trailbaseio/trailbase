# Logs Workspace Refresh Implementation Plan

> **REQUIRED SUB-SKILL:** Use the executing-plans skill to implement this plan task-by-task.

**Goal:** Build a compact request-investigation workspace with independent activity insights, a dense log table, and accessible request details while preserving existing APIs and URL behavior.

**Architecture:** `LogsPage` remains the URL/query/table owner. Pure presentation helpers make log values deterministic and testable, `LogsInsights` owns Chart.js/MapLibre lifecycle, and `LogDetailsSheet` renders the selected record. Existing shared primitives and backend contracts are reused; no dependency or backend changes are allowed.

**Tech Stack:** SolidJS, TypeScript, TanStack Solid Query/Table, Kobalte Sheet, Tailwind CSS, Chart.js, MapLibre GL, Vitest, Solid Testing Library.

---

## Constraints

- Work directly on `feat/admin-ui-refresh`; the user previously declined a worktree.
- Preserve `/_/admin/logs`, `/logs/list`, `/logs/stats`, FExpr syntax, URL keys, sorting, cursor behavior, Chart.js, MapLibre, and manual refresh.
- Do not add auto-refresh, live tailing, saved filters, export, request replay, deletion, backend changes, or dependencies.
- Implement sequentially with a failing test before each behavior change.
- After each implementation task, run exact-spec review before code-quality review.

### Task 1: Add deterministic log presentation helpers

**Files:**
- Create: `crates/assets/js/admin/src/components/logs/logs.ts`
- Create: `crates/assets/js/admin/tests/logs-workspace.test.ts`

**Step 1: Write failing helper tests**

Cover the exact public helpers:

```ts
expect(formatLogTimestamp(1_700_000_000)).toEqual({
  date: "2023-11-14",
  time: "22:13:20.000",
  iso: "2023-11-14T22:13:20.000Z",
});
expect(formatLogLatency(0.882209)).toBe("0.88 ms");
expect(formatLogLatency(29.313958)).toBe("29.3 ms");
expect(formatLogLatency(1_250)).toBe("1.25 s");
expect(logStatusTone(204)).toBe("success");
expect(logStatusTone(302)).toBe("muted");
expect(logStatusTone(404)).toBe("warning");
expect(logStatusTone(503)).toBe("destructive");
expect(logClientLabel(logWithCity)).toBe("Paris, FR");
expect(logClientLabel(logWithoutGeo)).toBe("::1");
```

Also cover null user IDs, country-only GeoIP, and boundary statuses/latencies.

**Step 2: Verify RED**

Run:

```bash
pnpm --dir crates/assets/js/admin exec vitest run tests/logs-workspace.test.ts
```

Expected: FAIL because `@/components/logs/logs` does not exist.

**Step 3: Implement the minimum pure helpers**

Export:

```ts
export type LogStatusTone = "success" | "muted" | "warning" | "destructive";
export function formatLogTimestamp(created: number): {
  date: string;
  time: string;
  iso: string;
};
export function formatLogLatency(latencyMs: number): string;
export function logStatusTone(status: number): LogStatusTone;
export function logClientLabel(log: LogJson): string;
```

Use `Date#toISOString`, simple numeric thresholds, and existing `LogJson`; do not introduce classes or formatter configuration.

**Step 4: Verify GREEN**

Run the focused test and formatting for both files.

**Step 5: Commit**

```bash
git add crates/assets/js/admin/src/components/logs/logs.ts crates/assets/js/admin/tests/logs-workspace.test.ts
git commit -m "feat(admin): model log request presentation"
```

### Task 2: Build accessible request inspection

**Files:**
- Create: `crates/assets/js/admin/src/components/logs/LogDetailsSheet.tsx`
- Create: `crates/assets/js/admin/tests/log-details-sheet.test.tsx`
- Modify: `crates/assets/js/admin/src/components/Table.tsx`
- Modify: `crates/assets/js/admin/tests/table.test.tsx`

**Step 1: Write failing detail-sheet tests**

Render one complete `LogJson` and assert:

- Accessible title contains method and URL.
- Request, Timing, Client and location, Identity, and Metadata groups exist.
- All existing fields are present: `id`, `created`, `status`, `method`, `url`, `latency_ms`, `client_ip`, `client_geoip_cc`, `client_geoip_city`, `referer`, `user_agent`, and `user_id`.
- Null/missing values render as `—`.
- Copy buttons call `copyToClipboard` with the value but feedback only says e.g. `Client IP copied`.
- Closing calls the supplied close callback.
- Desktop width is capped while the base width remains mobile-safe.

**Step 2: Verify RED**

Run:

```bash
pnpm --dir crates/assets/js/admin exec vitest run tests/log-details-sheet.test.tsx
```

Expected: FAIL because the component does not exist.

**Step 3: Implement `LogDetailsSheet`**

Use controlled Kobalte primitives:

```tsx
<Sheet open={props.log !== undefined} onOpenChange={...}>
  <SheetContent class="w-full sm:max-w-[520px]">
    <SheetHeader>
      <SheetTitle>{method} {url}</SheetTitle>
      <SheetDescription>Request details</SheetDescription>
    </SheetHeader>
    {/* semantic grouped definition lists */}
  </SheetContent>
</Sheet>
```

Keep copy behavior local and reuse `copyToClipboard`. Do not echo copied values into notifications.

**Step 4: Add the row-trigger contract to shared `Table`**

Extend the optional callback without breaking existing callers:

```ts
onRowClick?: (
  idx: number,
  row: TData,
  trigger: HTMLTableRowElement,
) => void;
```

Pass `event.currentTarget` for pointer and keyboard activation. Add a shared-table test proving the originating row element is supplied and remains keyboard operable.

**Step 5: Verify GREEN**

Run both focused test files, TypeScript, and formatting.

**Step 6: Commit**

```bash
git add crates/assets/js/admin/src/components/logs/LogDetailsSheet.tsx crates/assets/js/admin/src/components/Table.tsx crates/assets/js/admin/tests/log-details-sheet.test.tsx crates/assets/js/admin/tests/table.test.tsx
git commit -m "feat(admin): add log request inspection"
```

### Task 3: Extract and compact activity insights

**Files:**
- Create: `crates/assets/js/admin/src/components/logs/LogsInsights.tsx`
- Create: `crates/assets/js/admin/tests/logs-insights.test.tsx`
- Modify: `crates/assets/js/admin/src/components/logs/LogsPage.tsx`

**Step 1: Write failing insights tests**

Mock Chart.js and MapLibre and verify:

- Activity has an accessible expand/collapse control.
- Expanded desktop content is compact and contains a labeled request-rate canvas.
- Mobile starts collapsed when `window.innerWidth` is `390` before render.
- Country data renders the map surface and an accessible geographic summary.
- Missing country data renders inline GeoIP setup guidance only when geography is requested; no modal opens.
- A statistics error renders a Retry action without affecting sibling content.
- Chart instances are destroyed before replacement and on cleanup.
- Map instances are removed before replacement and on cleanup.
- Chart color is read from `--primary` rather than a fixed hex value.

**Step 2: Verify RED**

Run:

```bash
pnpm --dir crates/assets/js/admin exec vitest run tests/logs-insights.test.tsx
```

Expected: FAIL because `LogsInsights` does not exist.

**Step 3: Move existing visualization behavior with minimal changes**

Move `WorldMap`, `MapOverlay`, `LogsGraph`, map construction, GeoJSON constants, and development fixture augmentation from `LogsPage.tsx` into `LogsInsights.tsx`.

Required corrections during the move:

- Replace global `id="graph"` and `id="map"` lookup with element refs.
- Use a 180px desktop visualization height.
- Destroy/remove old instances before rebuilding and during cleanup.
- Read semantic CSS colors from `getComputedStyle(document.documentElement)`.
- Catch synchronous construction errors and listen for MapLibre error events so visualization failures remain local.
- Keep the existing OpenFreeMap style, GeoJSON source, projection, controls, hover behavior, and development data behavior.

Use `createIsMobile()` only to choose the initial expansion state; do not persist it.

**Step 4: Verify GREEN**

Run the focused test, TypeScript, and formatting.

**Step 5: Commit**

```bash
git add crates/assets/js/admin/src/components/logs/LogsInsights.tsx crates/assets/js/admin/src/components/logs/LogsPage.tsx crates/assets/js/admin/tests/logs-insights.test.tsx
git commit -m "feat(admin): compact log activity insights"
```

### Task 4: Rebuild `LogsPage` as the query-owned request workspace

**Files:**
- Modify: `crates/assets/js/admin/src/components/logs/LogsPage.tsx`
- Create: `crates/assets/js/admin/tests/logs-workspace-ui.test.tsx`

**Step 1: Write failing query/state tests**

Mock `useSearchParams`, `useQuery`, `fetchLogs`, and `fetchStats`. Verify:

- The list query key contains page size, page index, filter, and sorting.
- The stats query key contains the active filter.
- Applying/clearing a filter resets `pageIndex` and `pageSize` while preserving the new filter.
- Sorting resets pagination.
- List or statistics failures never call a reset that clears URL state.
- Refresh calls both query refetch functions and exposes a busy/disabled state while either refresh is pending.
- Statistics failure leaves the table and filter usable.
- List failure keeps the toolbar visible and renders Retry.

**Step 2: Verify RED**

Run:

```bash
pnpm --dir crates/assets/js/admin exec vitest run tests/logs-workspace-ui.test.tsx
```

Expected: FAIL against the current coupled/resetting behavior.

**Step 3: Implement stable query ownership**

Keep `LogsPage` as the only owner of both queries. Remove error-path calls to `reset()`. Use explicit primitive query-key segments rather than function/object identity where practical:

```ts
queryKey: ["logs", pageSize(), pageIndex(), filter() ?? "", order]
queryKey: ["log-stats", filter() ?? ""]
```

Await both manual refetches and retain current data during refresh. Sanitize user-facing error copy.

**Step 4: Write failing presentation tests**

Verify:

- Header shows `Logs`, filtered request count, and labeled Refresh.
- Table columns are exactly Time, Status, Method, Request, Latency, Client, and User.
- Status/method badges, formatted latency, dominant URL, client secondary data, and null user presentation are correct.
- Initial loading uses stable rows while controls remain present.
- No data without a filter says no requests have been recorded.
- No data with a filter says no requests match and offers Clear filter.
- Filter syntax disclosure includes valid examples without adding another filter state.
- Table container owns horizontal overflow.

**Step 5: Implement the dense workspace**

Rebuild the column definitions around helpers from Task 1. Keep raw numeric accessors for server sorting while formatting cells. Use existing `Header`, `FilterBar`, `Table`, `Badge`, `Button`, and `Callout` primitives. Keep pagination above the table so results do not jump during loading.

Wire row activation to `LogDetailsSheet`, retaining the `HTMLTableRowElement` trigger. On sheet close, clear selection and restore focus with `queueMicrotask(() => trigger.focus())`.

**Step 6: Verify GREEN**

Run all Logs tests, shared table tests, TypeScript, formatting, and modified-file ESLint.

**Step 7: Commit**

```bash
git add crates/assets/js/admin/src/components/logs/LogsPage.tsx crates/assets/js/admin/tests/logs-workspace-ui.test.tsx
git commit -m "feat(admin): complete logs request workspace"
```

### Task 5: Review, full validation, and browser acceptance

**Files:**
- Modify only files required by concrete review or browser findings.
- Add regression tests before fixing any discovered defect.

**Step 1: Run exact-spec review**

Review the complete Logs range against:

- `docs/plans/2026-08-30-logs-workspace-design.md`
- `docs/plans/2026-08-30-logs-workspace.md`

Block on missing functionality, API/URL regressions, misleading statistics, responsive overflow, lost fields, or accessibility failures.

**Step 2: Run code-quality review**

Review Solid reactivity, cursor mutation, query keys, Sheet focus, copy confidentiality, Chart.js/MapLibre cleanup, semantic colors, test fidelity, and unnecessary abstractions.

**Step 3: Run fresh automated validation**

```bash
pnpm --dir crates/assets/js/admin check:format
pnpm --dir crates/assets/js/admin check
pnpm --dir crates/assets/js/admin build
git diff --check
git status --short
```

Expected: all tests and builds pass; only documented unrelated pre-existing warnings may remain.

**Step 4: Run browser acceptance at desktop `1440×900`**

Validate in light and dark themes:

- Full-width workspace and sidebar.
- Compact Activity graph and GeoIP map.
- Activity collapse and inline missing/error states.
- Request count and manual refresh.
- Filter submission, clearing, syntax help, URL state, sorting, and pagination.
- Dense table hierarchy and contained overflow.
- Row click/keyboard opening.
- Complete detail sheet, copy actions, close, and focus restoration.
- Initial empty, filtered empty, loading, list error, and statistics error states.
- Console and network output.

**Step 5: Run browser acceptance at mobile `390×844`**

Validate Activity starts collapsed, header/filter controls wrap, the table scrolls internally without body overflow, the detail sheet uses the viewport width, and focus/touch targets remain usable. Restore `1440×900` afterward.

**Step 6: Fix findings with TDD and re-run all gates**

Every non-trivial browser/review fix must begin with a failing regression test.

**Step 7: Commit final corrections**

```bash
git add <reviewed-files>
git commit -m "fix(admin): complete logs workspace validation"
```

Skip this commit if no corrections are required.

## Completion criteria

- Approved request-inspector design is implemented without backend or dependency changes.
- Manual refresh and existing filter/URL/API contracts remain intact.
- Every `LogJson` field is inspectable.
- List and insights failures are independent and preserve user state.
- Desktop light/dark and mobile browser acceptance pass.
- Exact-spec and code-quality reviews approve the complete change set.
- Fresh tests, TypeScript, ESLint, formatting, production build, and diff checks pass.
