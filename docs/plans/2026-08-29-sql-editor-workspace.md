# SQL Editor Workspace Implementation Plan

> **REQUIRED SUB-SKILL:** Use the executing-plans skill to implement this plan task-by-task.

**Goal:** Rebuild the admin SQL Editor as an execution-focused workspace with a searchable saved-query explorer, clear editor actions, structured result feedback, pagination, and responsive Editor/Results tabs.

**Architecture:** Keep all existing CodeMirror, TanStack Query, local-storage, schema, attached-database, execution, and dirty-navigation behavior in `EditorPage.tsx`. Add small exported pure helpers for testable filtering, CSV generation, result presentation, and client-side pagination. Reuse the existing sidebar, dropdown-menu, tabs, table, badge, callout, dialog, and button primitives; add no dependencies or backend changes.

**Tech Stack:** SolidJS, Kobalte, CodeMirror, TanStack Solid Query/Table, Nanostores persistent atoms, Tailwind CSS, Vitest, Testing Library.

---

## Existing behavior that must remain intact

- Route: `/_/admin/editor`
- Saved script key: `scripts`
- Editor UI key: `editor_ui_state`
- Schema-aware SQLite autocomplete
- Attached database selection
- `Ctrl/Cmd+S` save shortcut
- `Ctrl/Cmd+Enter` execute shortcut
- Dirty-state protection when switching scripts or navigating away
- Cached result stored with each saved script
- Existing SQL execution API and error toast
- Migration and large-result safety messaging

## Task 1: Define testable workspace behavior

**Files:**

- Create: `crates/assets/js/admin/tests/editor-workspace.test.ts`
- Modify: `crates/assets/js/admin/src/components/editor/EditorPage.tsx`

### Step 1: Write failing helper tests

Add focused tests for:

```ts
import {
  buildCsv,
  filterSavedQueries,
  paginateResultRows,
  resultPresentation,
} from "@/components/editor/EditorPage";

describe("SQL editor workspace", () => {
  it("filters saved queries case-insensitively", () => {
    const scripts = [
      { name: "Active users", contents: "SELECT 1" },
      { name: "Recent posts", contents: "SELECT 2" },
    ];
    expect(filterSavedQueries(scripts, "USER")).toEqual([
      scripts[0],
    ]);
  });

  it("escapes CSV headers and values", () => {
    expect(buildCsv(responseWithQuotesAndCommas)).toBe(
      '"full,name"\n"Ada ""Lovelace"""',
    );
  });

  it("describes cached, successful, empty, and failed results", () => {
    expect(resultPresentation(undefined, true).label).toBe("No result");
    expect(resultPresentation(success, true).label).toBe("Cached result");
    expect(resultPresentation(success, false).label).toBe("Success");
    expect(resultPresentation(empty, false).label).toBe("No rows");
    expect(resultPresentation(failure, false).label).toBe("Error");
  });

  it("slices result rows without losing the total", () => {
    expect(paginateResultRows([0, 1, 2, 3], 1, 2)).toEqual([2, 3]);
  });
});
```

Use real `QueryResponse`/`ExecutionResult` shapes from the generated bindings and execution API types.

### Step 2: Run the tests and verify red

Run:

```bash
pnpm --dir crates/assets/js/admin exec vitest run tests/editor-workspace.test.ts
```

Expected: FAIL because the helpers are not exported.

### Step 3: Add the minimum pure helpers

In `EditorPage.tsx`:

- Export `Script`.
- Export the existing `buildCsv`.
- Add `filterSavedQueries(scripts, search)` using trimmed, case-insensitive `String.includes`.
- Add `paginateResultRows(rows, pageIndex, pageSize)` using `Array.slice`.
- Add `resultPresentation(result, cached)` returning only the label and semantic variant needed by the UI.

Do not add a general workspace-state abstraction.

### Step 4: Run focused tests and checks

```bash
pnpm --dir crates/assets/js/admin exec vitest run tests/editor-workspace.test.ts
pnpm --dir crates/assets/js/admin exec tsc --noEmit --skipLibCheck
pnpm --dir crates/assets/js/admin exec eslint src/components/editor/EditorPage.tsx tests/editor-workspace.test.ts
```

Expected: tests pass; no TypeScript or ESLint errors.

### Step 5: Commit

```bash
git add crates/assets/js/admin/src/components/editor/EditorPage.tsx crates/assets/js/admin/tests/editor-workspace.test.ts
git commit -m "test(admin): define sql editor workspace behavior"
```

## Task 2: Rebuild the saved-query explorer

**Files:**

- Modify: `crates/assets/js/admin/src/components/editor/EditorPage.tsx`
- Modify: `crates/assets/js/admin/tests/editor-workspace.test.ts`

### Step 1: Extend the filtering tests

Cover trimmed empty search, no matches, and stable original order. Run the focused test and confirm any missing behavior fails.

### Step 2: Build the explorer

Replace the current `EditorSidebar` presentation with:

- Header row: `Saved queries`, visible count, and icon-only **New query** action
- Accessible search input: `Search saved queries`
- Compact selected query rows
- Dirty dot plus screen-reader text for the active dirty query
- Per-row overflow menu containing Rename and Delete
- Rename dialog reusing current update behavior
- Lightweight delete confirmation dialog
- Empty state with **Create query**
- No-results state with **Clear search**

Keep query switching routed through `switchToScript` so the dirty guard remains the single source of truth. Search must never change the selected query.

### Step 3: Isolate sidebar persistence

Update the editor provider:

```tsx
<SidebarProvider
  cookieName="sql-explorer:state"
  style={{ "--sidebar-width": "15rem" }}
>
```

Keep `collapsible="offcanvas"`, the shared fully-hidden collapsed behavior, and the directional trigger.

### Step 4: Verify

```bash
pnpm --dir crates/assets/js/admin exec vitest run tests/editor-workspace.test.ts tests/sidebar.test.tsx
pnpm --dir crates/assets/js/admin exec tsc --noEmit --skipLibCheck
pnpm --dir crates/assets/js/admin exec eslint src/components/editor/EditorPage.tsx tests/editor-workspace.test.ts
```

Manually confirm New, search, select, rename, delete/cancel, empty, no-results, dirty query, and collapsed explorer states.

### Step 5: Commit

```bash
git add crates/assets/js/admin/src/components/editor/EditorPage.tsx crates/assets/js/admin/tests/editor-workspace.test.ts
git commit -m "feat(admin): organize saved sql queries"
```

## Task 3: Build the query header and editor action hierarchy

**Files:**

- Modify: `crates/assets/js/admin/src/components/editor/EditorPage.tsx`

### Step 1: Replace the generic page header

Create a local sticky editor header following the Tables workspace pattern:

- Directional saved-query explorer trigger
- Breadcrumb: `SQL Editor › query name`
- Visible unsaved indicator and accessible dirty text
- Attached-database selector on the right
- Help action in the right action group

Do not change attached database values or API arguments.

### Step 2: Compact the migration warning

Keep the existing persistent dismissal key and warning copy. Restyle it as a compact dismissible banner above the editor. Give the dismiss button an explicit accessible label.

### Step 3: Reframe CodeMirror and actions

- Put CodeMirror in a bordered editor surface with a restrained desktop minimum height.
- Preserve schema completion, theme, line numbers, and existing keymaps.
- Place Save and Execute in one compact action row.
- Save is secondary; Execute is primary.
- Display shortcut text on desktop and shorter labels on mobile.
- Keep focus behavior and current execute-all-editor behavior.

### Step 4: Verify

```bash
pnpm --dir crates/assets/js/admin exec vitest run tests/editor-workspace.test.ts
pnpm --dir crates/assets/js/admin exec tsc --noEmit --skipLibCheck
pnpm --dir crates/assets/js/admin exec eslint src/components/editor/EditorPage.tsx
pnpm --dir crates/assets/js/admin exec prettier -c src/components/editor/EditorPage.tsx
```

Manually verify Save, Execute, attached databases, Help, migration-warning dismissal, dirty label, and keyboard shortcuts.

### Step 5: Commit

```bash
git add crates/assets/js/admin/src/components/editor/EditorPage.tsx
git commit -m "feat(admin): focus the sql query workspace"
```

## Task 4: Add explicit execution feedback

**Files:**

- Modify: `crates/assets/js/admin/src/components/editor/EditorPage.tsx`
- Modify: `crates/assets/js/admin/tests/editor-workspace.test.ts`

### Step 1: Extend result-presentation tests

Add a pending/stale case and verify the helper produces the exact semantic labels used by the UI.

### Step 2: Wire TanStack Query lifecycle state

Use the existing `executionResult` query state:

- Disable Execute while `isFetching`.
- Show an inline spinner/progress label.
- Prevent duplicate submissions through the disabled action.
- Keep the previous response visible during refetch.
- Mark that response `Running…`/stale until the new response arrives.
- Preserve script result caching and the existing error toast.

Do not change `executeSql`, query keys, attached database handling, or result persistence.

### Step 3: Improve save feedback

Keep the existing save flow, clear dirty state, and use concise toast copy (`Query saved`). Do not blur or recreate CodeMirror after saving.

### Step 4: Verify

```bash
pnpm --dir crates/assets/js/admin exec vitest run tests/editor-workspace.test.ts
pnpm --dir crates/assets/js/admin exec tsc --noEmit --skipLibCheck
```

Manually execute success, error, and no-data statements and confirm repeated clicks cannot duplicate execution.

### Step 5: Commit

```bash
git add crates/assets/js/admin/src/components/editor/EditorPage.tsx crates/assets/js/admin/tests/editor-workspace.test.ts
git commit -m "feat(admin): clarify sql execution feedback"
```

## Task 5: Rebuild results and add client-side pagination

**Files:**

- Modify: `crates/assets/js/admin/src/components/editor/EditorPage.tsx`
- Modify: `crates/assets/js/admin/tests/editor-workspace.test.ts`

### Step 1: Complete pagination tests

Cover first page, final partial page, out-of-range page reset/clamp behavior, and a changed page size.

### Step 2: Build the result header

Show:

- Semantic status badge: Success, Error, No rows, Cached result, or Running
- Returned row count
- Execution timestamp when present
- **Copy CSV** action with accessible label and disabled state when no tabular data exists

Keep errors inline, selectable, and copyable. Replace plain `No data` with a dedicated state that distinguishes statements returning no tabular result from a query returning zero rows.

### Step 3: Paginate rendering

Add local `PaginationState` to `ResultViewImpl` and pass sliced rows to `buildTable`:

```ts
const visibleRows = () =>
  paginateResultRows(props.data.rows, pagination().pageIndex, pagination().pageSize);

buildTable({
  columns: columnDefs(props.data),
  data: visibleRows(),
  rowCount: props.data.rows.length,
  pagination: pagination(),
  onPaginationChange: setPagination,
});
```

Render the shared table in dense mode with bottom pagination. Reset to page zero when result identity or row count changes. Keep the `LIMIT` safety warning because pagination is client-side only.

### Step 4: Verify

```bash
pnpm --dir crates/assets/js/admin exec vitest run tests/editor-workspace.test.ts tests/table.test.tsx
pnpm --dir crates/assets/js/admin exec tsc --noEmit --skipLibCheck
pnpm --dir crates/assets/js/admin exec eslint src/components/editor/EditorPage.tsx tests/editor-workspace.test.ts
```

Manually verify wide columns, zero rows, one row, multiple pages, page-size changes, cached output, execution errors, and CSV copy.

### Step 5: Commit

```bash
git add crates/assets/js/admin/src/components/editor/EditorPage.tsx crates/assets/js/admin/tests/editor-workspace.test.ts
git commit -m "feat(admin): structure sql query results"
```

## Task 6: Add mobile Editor/Results tabs and responsive polish

**Files:**

- Modify: `crates/assets/js/admin/src/components/editor/EditorPage.tsx`
- Modify: `crates/assets/js/admin/tests/editor-workspace.test.ts`

### Step 1: Add minimal tab-state tests

Export a narrow `EditorWorkspaceTab = "editor" | "results"` type and a normalizer only if needed by component state. Test only real branch behavior; do not create URL state because the approved design does not require it.

### Step 2: Implement responsive presentation

- Desktop/tablet: editor and results remain vertically stacked and visible.
- Mobile: render accessible Editor and Results tabs.
- Start on Editor.
- Switch to Results after an execution attempt resolves.
- Preserve query contents, dirty state, attached databases, and result data when switching tabs.
- Keep the query explorer as the existing mobile sheet.
- Ensure no horizontal body overflow; only the result grid may scroll horizontally.

### Step 3: Polish states

Check long query names, many saved queries, long database names, narrow headers, dark/light contrast, focus rings, reduced-motion behavior, and loading/error boundaries.

### Step 4: Verify

```bash
pnpm --dir crates/assets/js/admin exec vitest run tests/editor-workspace.test.ts tests/sidebar.test.tsx tests/table.test.tsx
pnpm --dir crates/assets/js/admin exec tsc --noEmit --skipLibCheck
pnpm --dir crates/assets/js/admin exec eslint src/components/editor/EditorPage.tsx tests/editor-workspace.test.ts
pnpm --dir crates/assets/js/admin exec prettier -c src/components/editor/EditorPage.tsx tests/editor-workspace.test.ts
```

Manually verify desktop, collapsed desktop, tablet, and mobile widths in dark and light themes.

### Step 5: Commit

```bash
git add crates/assets/js/admin/src/components/editor/EditorPage.tsx crates/assets/js/admin/tests/editor-workspace.test.ts
git commit -m "fix(admin): polish the responsive sql workspace"
```

## Task 7: Verify preserved workflows and release quality

**Files:**

- Modify only if verification reveals a defect.

### Step 1: Run the complete admin suite

```bash
pnpm --dir crates/assets/js/admin format
pnpm --dir crates/assets/js/admin check:format
pnpm --dir crates/assets/js/admin check
pnpm --dir crates/assets/js/admin build
git diff --check
```

Expected: formatting passes; TypeScript and ESLint report no errors; all Vitest tests pass; production build succeeds.

### Step 2: Verify browser workflows

At `http://localhost:3000/_/admin/editor`, verify:

1. Create a query.
2. Search and clear search.
3. Rename a query.
4. Delete and cancel deletion.
5. Edit and save with button and shortcut.
6. Execute success, zero-row, no-tabular-data, and error statements.
7. Attach/detach databases.
8. Copy CSV containing commas and quotes.
9. Page through a multi-row result.
10. Switch queries with unsaved changes and exercise Save/Discard/Cancel.
11. Collapse and restore the saved-query explorer.
12. Confirm the main sidebar state remains independent.
13. Verify mobile Editor/Results switching.
14. Confirm no console exceptions, failed resources, or body-level horizontal overflow.

### Step 3: Review scope

Confirm the implementation changes only admin frontend files and tests. Confirm no dependency, lockfile, generated binding, backend, route, or storage-key changes.

### Step 4: Commit verification fixes separately

If verification finds a defect, write a focused failing test, apply the smallest fix, rerun the relevant checks, and commit with a `fix(admin): ...` message. Do not bundle unrelated cleanup.
