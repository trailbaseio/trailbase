# Tables Workspace Implementation Plan

> **REQUIRED SUB-SKILL:** Use the executing-plans skill to implement this plan task-by-task.

**Goal:** Turn the Tables screen into a professional, task-oriented workspace with a searchable resource explorer and separate Data, Structure, and API views while preserving every existing action.

**Architecture:** Keep the existing `/table/:table?` route, API calls, forms, and TanStack table model. Restructure `TablesPage.tsx` around the resource explorer and `TablePane.tsx` around controlled Kobalte tabs whose value is stored in `?tab=`; reuse existing sheets and destructive confirmation flows rather than rewriting business logic. Add only one missing shared primitive—an accessible Kobalte dropdown menu—for the compact resource overflow menu.

**Tech Stack:** SolidJS 1.9, `@solidjs/router`, Kobalte 0.13, TanStack Solid Query/Table, Tailwind CSS 4, Vitest, Solid Testing Library.

**Design:** `docs/plans/2026-08-28-tables-workspace-design.md`

---

### Task 1: Add testable workspace and explorer state

**Files:**
- Modify: `crates/assets/js/admin/src/components/tables/TablesPage.tsx:54-93`
- Modify: `crates/assets/js/admin/src/components/tables/TablePane.tsx:978-1035`
- Create: `crates/assets/js/admin/tests/tables-workspace.test.ts`

**Step 1: Write the failing tests**

Create tests for the two pieces of behavior that should not depend on rendering or API mocks:

```ts
import { describe, expect, it } from "vitest";
import {
  filterExplorerResources,
  resourceSchemaName,
} from "@/components/tables/TablesPage";
import { normalizeWorkspaceTab } from "@/components/tables/TablePane";

describe("Tables workspace", () => {
  it("defaults invalid workspace tabs to data", () => {
    expect(normalizeWorkspaceTab(undefined)).toBe("data");
    expect(normalizeWorkspaceTab("structure")).toBe("structure");
    expect(normalizeWorkspaceTab("api")).toBe("api");
    expect(normalizeWorkspaceTab("unknown")).toBe("data");
  });

  it("filters resources case-insensitively by qualified name", () => {
    const resources = [table("post", "main"), table("UserProfile", "auth")];
    expect(filterExplorerResources(resources, "profile")).toEqual([
      resources[1],
    ]);
    expect(filterExplorerResources(resources, "AUTH.")).toEqual([
      resources[1],
    ]);
  });

  it("uses a stable label for resources without a schema", () => {
    expect(resourceSchemaName(table("post", undefined))).toBe("main");
  });
});
```

Use the generated `Table` type or a minimal typed fixture builder in the test; do not introduce production fixture helpers.

**Step 2: Run the test to verify it fails**

Run:

```bash
pnpm --dir crates/assets/js/admin exec vitest run tests/tables-workspace.test.ts
```

Expected: FAIL because the three exported helpers do not exist.

**Step 3: Add the minimal pure helpers**

In `TablesPage.tsx`, export:

```ts
export function resourceSchemaName(resource: Table | View): string {
  return resource.name.database_schema || "main";
}

export function filterExplorerResources<T extends Table | View>(
  resources: T[],
  query: string,
): T[] {
  const normalized = query.trim().toLocaleLowerCase();
  if (!normalized) return resources;
  return resources.filter((resource) =>
    prettyFormatQualifiedName(resource.name)
      .toLocaleLowerCase()
      .includes(normalized),
  );
}
```

In `TablePane.tsx`, export the narrow tab type and parser:

```ts
export type WorkspaceTab = "data" | "structure" | "api";

export function normalizeWorkspaceTab(value: string | undefined): WorkspaceTab {
  return value === "structure" || value === "api" ? value : "data";
}
```

**Step 4: Run the focused test**

Expected: PASS.

**Step 5: Commit**

```bash
git add crates/assets/js/admin/src/components/tables/TablesPage.tsx \
  crates/assets/js/admin/src/components/tables/TablePane.tsx \
  crates/assets/js/admin/tests/tables-workspace.test.ts
git commit -m "test(admin): define tables workspace state"
```

---

### Task 2: Rebuild the table explorer

**Files:**
- Modify: `crates/assets/js/admin/src/components/tables/TablesPage.tsx:95-215,232-356`
- Test: `crates/assets/js/admin/tests/tables-workspace.test.ts`

**Step 1: Extend the failing explorer tests**

Add cases proving that:

- Empty search returns the original ordered resources.
- Qualified-name search matches schema and resource name.
- Grouping produces stable schema labels in first-seen order.

Add one pure helper only if grouping cannot remain a short memo inside the component:

```ts
export function groupExplorerResources(resources: (Table | View)[]) {
  return [...Map.groupBy(resources, resourceSchemaName)];
}
```

Use the standard `Map.groupBy` only if the project TypeScript target supports it; otherwise use one short `Map` loop rather than a dependency or generic grouping utility.

**Step 2: Run the focused test and verify the new grouping test fails**

**Step 3: Replace the explorer’s flat list**

Update `TablePickerSidebar` to include:

- A compact header row with `Tables`, visible-resource count, and Add Table icon button.
- A `TextFieldInput` with `aria-label="Search tables and views"`.
- Existing hidden-resource toggle with a clear tooltip and pressed-state styling.
- Schema groups using `SidebarGroupLabel`.
- The existing table, view, virtual-table, hidden, selected, navigation, and mobile-close behavior.
- A no-results message with a Clear search action.

Keep search local with `createSignal("")`; do not persist it. Remove currently unused `allTables` and `schemaRefetch` props from `TablePickerSidebar`.

Set the nested explorer width through the existing sidebar CSS variable/provider rather than adding a resizable dependency or new layout system. Give the explorer its own persistence cookie name so it cannot affect the application shell.

**Step 4: Run the focused test and full typecheck**

```bash
pnpm --dir crates/assets/js/admin exec vitest run tests/tables-workspace.test.ts
pnpm --dir crates/assets/js/admin exec tsc --noEmit --skipLibCheck
```

Expected: PASS.

**Step 5: Commit**

```bash
git add crates/assets/js/admin/src/components/tables/TablesPage.tsx \
  crates/assets/js/admin/tests/tables-workspace.test.ts
git commit -m "feat(admin): organize the table explorer"
```

---

### Task 3: Add the resource header, overflow menu, and URL-backed tabs

**Files:**
- Create: `crates/assets/js/admin/src/components/ui/dropdown-menu.tsx`
- Modify: `crates/assets/js/admin/src/components/DestructiveActionButton.tsx`
- Modify: `crates/assets/js/admin/src/components/tables/TablePane.tsx:290-490,978-1168`
- Modify: `crates/assets/js/admin/tests/tables-workspace.test.ts`

**Step 1: Add failing tab-query tests**

Add a pure helper that produces the next query object without losing unrelated state:

```ts
expect(
  workspaceTabSearchParams(
    { filter: "id > 2", pageSize: "50", tab: "structure" },
    "api",
  ),
).toEqual({ filter: "id > 2", pageSize: "50", tab: "api" });

expect(workspaceTabSearchParams({ filter: "x" }, "data")).toEqual({
  filter: "x",
  tab: undefined,
});
```

Run the focused test and confirm it fails.

**Step 2: Add the minimal accessible dropdown primitive**

Wrap Kobalte’s `DropdownMenu.Root`, `Trigger`, `Portal`, `Content`, `Item`, `Separator`, and `ItemLabel` in `ui/dropdown-menu.tsx`. Follow the installed Kobalte API: Root controls open state, Item uses `onSelect`, and Content is portalled. Style only the trigger-independent menu surface and items.

Do not add submenus, checkbox items, radio groups, or animation infrastructure.

**Step 3: Make destructive triggers styleable**

Add optional `variant` and `class` trigger props to `DestructiveActionButton`, defaulting to its current destructive appearance. Preserve the confirmation dialog and action error toast exactly.

**Step 4: Replace `TableHeader`**

Build a sticky resource header directly in `TablePane.tsx` with:

- Nested explorer `SidebarTrigger`.
- Breadcrumb text: Tables / schema / name.
- `Badge` for Table, View, or Virtual Table.
- Compact counts available from existing schema/record data.
- Refresh action on Data.
- Alter action on Structure when supported.
- Configure API action on API when supported.
- More menu containing SQL Schema and Delete Table.

Keep existing SQL schema dialog, drop API call, config refetch, schema refetch, postgres guards, hidden-resource guards, and confirmation message.

**Step 5: Add controlled tabs**

Extend `SearchParams` with `tab?: string`. Use existing `Tabs`, `TabsList`, `TabsTrigger`, and `TabsContent` with:

```tsx
<Tabs
  value={normalizeWorkspaceTab(searchParams.tab)}
  onChange={(tab) => setSearchParams(workspaceTabSearchParams(searchParams, tab))}
>
```

Ensure filter, pagination, fetch-error reset, and sorting updates preserve `tab`. Render temporary existing content in the matching tabs until later tasks refine it:

- Data: `RecordTable`
- Structure: `IndexTable` and `TriggerTable`
- API: current API configuration trigger/status

**Step 6: Run tests and typecheck**

Expected: focused tests and TypeScript PASS.

**Step 7: Commit**

```bash
git add crates/assets/js/admin/src/components/ui/dropdown-menu.tsx \
  crates/assets/js/admin/src/components/DestructiveActionButton.tsx \
  crates/assets/js/admin/src/components/tables/TablePane.tsx \
  crates/assets/js/admin/tests/tables-workspace.test.ts
git commit -m "feat(admin): add table workspace navigation"
```

---

### Task 4: Build the Data toolbar and contextual bulk actions

**Files:**
- Modify: `crates/assets/js/admin/src/components/FilterBar.tsx`
- Modify: `crates/assets/js/admin/src/components/tables/TablePane.tsx:570-800`
- Create: `crates/assets/js/admin/tests/filter-bar.test.tsx`

**Step 1: Write failing FilterBar interaction tests**

Test that:

- Submit calls `onSubmit` with the current value.
- Clear calls `onSubmit("")` and clears the visible input.
- The clear button is absent or disabled when empty.

Use Solid Testing Library and `fireEvent`; no API mocks are needed.

**Step 2: Run the focused test and verify failure**

```bash
pnpm --dir crates/assets/js/admin exec vitest run tests/filter-bar.test.tsx
```

Expected: FAIL because FilterBar has no clear control or reactive local value.

**Step 3: Upgrade `FilterBar` minimally**

Use a local signal synchronized from `props.initial`. Add optional props for a compact syntax-help element and keyboard-focus support. Render:

- Search icon inside or adjacent to the input.
- Apply button with text on desktop and accessible label at all widths.
- Clear button only when a value exists.
- Existing `example` support.

Install one document keydown listener on mount for `/`, ignore editable targets, focus the input, and remove the listener on cleanup.

**Step 4: Recompose `RecordTable` toolbar**

Move into one toolbar above the grid:

- FilterBar
- Refresh status/action
- Insert Row
- View controls containing blob encoding and development-only schema debug

Keep the existing SafeSheet insert/edit forms.

Replace the always-visible disabled Delete Rows button with a contextual selection bar shown only when `selectedRows().size > 0`. Keep the existing delete API call, error toast, refetch, and selection reset.

**Step 5: Run focused tests and typecheck**

Expected: PASS.

**Step 6: Commit**

```bash
git add crates/assets/js/admin/src/components/FilterBar.tsx \
  crates/assets/js/admin/src/components/tables/TablePane.tsx \
  crates/assets/js/admin/tests/filter-bar.test.tsx
git commit -m "feat(admin): focus the table data toolbar"
```

---

### Task 5: Make the record grid dense and state-aware

**Files:**
- Modify: `crates/assets/js/admin/src/components/Table.tsx:216-525`
- Modify: `crates/assets/js/admin/src/components/tables/TablePane.tsx:513-800`
- Create: `crates/assets/js/admin/tests/table.test.tsx`

**Step 1: Write failing shared-table tests**

Build a tiny TanStack table with `buildTable` and assert:

- A supplied empty-state message/action renders instead of the literal `Empty`.
- Bottom pagination is rendered after the table when requested.
- A clickable row is keyboard accessible and activates on Enter.

Avoid snapshots; assert roles and visible text.

**Step 2: Run the focused test and verify failure**

**Step 3: Add narrow opt-in props**

Extend `Table` with only the options needed by the record grid:

```ts
emptyState?: JSX.Element;
paginationPosition?: "top" | "bottom";
dense?: boolean;
```

Default to current behavior so Accounts, indexes, triggers, and other screens do not change unintentionally.

For `dense`:

- Use compact row/cell padding.
- Prevent technical-value wrapping.
- Apply truncation with full values still available through existing cell renderers/tooltips.
- Keep horizontal overflow in the RecordTable container.
- Make clickable rows focusable with Enter activation and a visible focus state.

Move column pin controls out of the always-visible corner into a compact header action shown on hover/focus. Do not build column resizing or persistence.

**Step 4: Pass Data-specific states**

From `RecordTable`, supply:

- `paginationPosition="bottom"`
- `dense`
- “No rows yet” with Insert first row when no filter exists
- “No rows match this filter” with Clear filter when a filter exists

Keep skeleton rows during loading. Use the existing query state rather than adding another loading signal.

**Step 5: Run focused tests and typecheck**

Expected: PASS.

**Step 6: Commit**

```bash
git add crates/assets/js/admin/src/components/Table.tsx \
  crates/assets/js/admin/src/components/tables/TablePane.tsx \
  crates/assets/js/admin/tests/table.test.tsx
git commit -m "feat(admin): refine the record data grid"
```

---

### Task 6: Build the Structure tab

**Files:**
- Modify: `crates/assets/js/admin/src/components/tables/TablePane.tsx:802-1005,1006-1168`
- Modify: `crates/assets/js/admin/tests/tables-workspace.test.ts`

**Step 1: Add failing structure-summary tests**

Export and test a small `tableStructureCounts(table, schemas)` helper returning column, index, and trigger counts for the selected qualified table. Include a similarly named table in another schema to prove qualified-name matching is preserved.

**Step 2: Run the focused test and verify failure**

**Step 3: Add the Structure presentation**

Create local components inside `TablePane.tsx`, not new files:

- `StructureSummary`
- `ColumnList`
- Restyled existing `IndexTable`
- Restyled existing `TriggerTable`

The summary shows resource type, qualified name, primary key, and counts using existing schema data. The column list shows name, type, nullable/default state, and PK/FK/unique badges derived from current `Column` options helpers.

Preserve:

- Alter table SafeSheet
- Index add/edit/select/delete flows
- Hidden and postgres restrictions
- Development debug dialogs
- Trigger SQL rendering

Replace trigger-only external documentation prose with a concise limitation callout and a route link to `/editor`; do not claim trigger creation is supported in the UI.

**Step 4: Run focused tests and typecheck**

Expected: PASS.

**Step 5: Commit**

```bash
git add crates/assets/js/admin/src/components/tables/TablePane.tsx \
  crates/assets/js/admin/tests/tables-workspace.test.ts
git commit -m "feat(admin): organize table structure tools"
```

---

### Task 7: Build the API tab around existing configuration

**Files:**
- Modify: `crates/assets/js/admin/src/components/tables/TablePane.tsx:276-425,1006-1190`
- Modify: `crates/assets/js/admin/src/components/tables/RecordApiSettings.tsx:276-290` only if a read-only selector must be exported
- Modify: `crates/assets/js/admin/tests/tables-workspace.test.ts`

**Step 1: Add failing API-summary tests**

Test a pure summary helper using existing `getRecordApis` and validation behavior:

- Supported table with no API reports disabled.
- Existing API reports enabled and returns its names.
- Virtual table reports requirement errors.

Do not duplicate Record API validation rules.

**Step 2: Run the focused test and verify failure**

**Step 3: Add the API overview**

Render a dedicated API tab with:

- Enabled/disabled status badge
- Existing API names
- Requirement errors in a Callout
- Resource endpoint pattern `/api/records/v1/<api-name>` for configured APIs
- Copy buttons using `navigator.clipboard.writeText`
- Configure API button opening the existing `RecordApiSettingsForm` inside its existing `SafeSheet`

Keep the form in a sheet for this iteration so dirty-state protection, validation, CRUD behavior, and complex form layout remain unchanged. The tab is the discoverable API home; it does not rewrite the form.

**Step 4: Run focused tests and typecheck**

Expected: PASS.

**Step 5: Commit**

```bash
git add crates/assets/js/admin/src/components/tables/TablePane.tsx \
  crates/assets/js/admin/src/components/tables/RecordApiSettings.tsx \
  crates/assets/js/admin/tests/tables-workspace.test.ts
git commit -m "feat(admin): add the table API workspace"
```

Only stage `RecordApiSettings.tsx` if it actually changed.

---

### Task 8: Finish responsive layout and remove wasted shell space

**Files:**
- Modify: `crates/assets/js/admin/src/App.tsx:25-47`
- Modify: `crates/assets/js/admin/src/components/tables/TablesPage.tsx:232-356`
- Modify: `crates/assets/js/admin/src/components/tables/TablePane.tsx:426-490,1006-1168`

**Step 1: Add a minimal shell regression assertion**

Extend an existing route/shell test only if practical. Assert that mobile global navigation still has an accessible trigger; do not create a brittle class snapshot.

If mounting `App` requires extensive API/auth mocking, document this as a manual visual check rather than introducing a large test harness.

**Step 2: Apply responsive rules**

- Hide the global branding header on desktop while retaining its mobile navigation trigger.
- Keep the application sidebar independently collapsible.
- Keep the explorer fixed-width on large desktop and off-canvas on mobile using the existing nested Sidebar.
- Ensure the resource header keeps the explorer trigger accessible.
- Make tabs horizontally scrollable.
- Collapse low-priority resource actions into the More menu at narrower widths.
- Keep data and schema tables horizontally scrollable; do not convert records to cards.
- Ensure only the workspace owns vertical scrolling to avoid nested full-page scrollbars.

Do not add resizable panels, breakpoint state persistence, or a new responsive abstraction.

**Step 3: Run automated checks**

```bash
pnpm --dir crates/assets/js/admin check:format
pnpm --dir crates/assets/js/admin check
pnpm --dir crates/assets/js/admin build
git diff --check
```

Expected: formatting passes, TypeScript and ESLint have no errors, all tests pass, production build succeeds, and diff check is clean. Existing unrelated lint/build warnings may remain documented.

**Step 4: Commit**

```bash
git add crates/assets/js/admin/src/App.tsx \
  crates/assets/js/admin/src/components/tables/TablesPage.tsx \
  crates/assets/js/admin/src/components/tables/TablePane.tsx \
  crates/assets/js/admin/tests
git commit -m "fix(admin): polish the responsive tables workspace"
```

---

### Task 9: Verify every preserved workflow visually

**Files:**
- Modify only files required by defects found during verification.

**Step 1: Start or reuse the development processes**

Verify the backend is the checkout build and Vite is serving the current branch:

- Admin UI: `http://localhost:3000/_/admin/table/`
- Backend API: `http://localhost:4000`

Do not compare port 3000 with port 4000 as old/new UI; both may contain branch assets.

**Step 2: Check desktop states with browser tooling**

At approximately 1440px and 1024px widths, verify:

- App sidebar expanded and collapsed
- Explorer visible, searchable, grouped, hidden-resource toggle, selection, and create table
- Data/Structure/API tabs update `?tab=` and preserve table/filter state
- Filter apply/clear, sorting, pagination, blob mode, refresh
- Insert, edit, select, delete rows
- Alter table and SQL schema
- Add/edit/delete indexes
- Trigger display and SQL Editor route
- API requirement states and configuration sheet
- Table deletion confirmation
- Loading, empty, filtered-empty, and fetch-error states where safely reproducible

**Step 3: Check mobile states**

At approximately 390px width, verify:

- Global navigation drawer
- Table explorer drawer
- Horizontally scrollable tabs and record grid
- Resource overflow actions
- Sheets remain usable without clipped footer actions

**Step 4: Fix only observed defects**

Use the smallest root-cause fix. Add a focused regression test for any logic or interaction defect found. Avoid opportunistic redesign outside Tables.

**Step 5: Run final verification**

```bash
pnpm --dir crates/assets/js/admin format
pnpm --dir crates/assets/js/admin check:format
pnpm --dir crates/assets/js/admin check
pnpm --dir crates/assets/js/admin build
git diff --check
git status --short --branch
```

Expected: all 21 existing tests plus new Tables tests pass, build succeeds, and the working tree contains only intended changes before the final commit.

**Step 6: Commit final fixes**

```bash
git add crates/assets/js/admin
git commit -m "fix(admin): close tables workspace review gaps"
```

Skip this commit if verification produces no changes.
