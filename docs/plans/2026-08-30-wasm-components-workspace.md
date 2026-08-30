# WASM Components Workspace Implementation Plan

> **REQUIRED SUB-SKILL:** Use the executing-plans skill to implement this plan task-by-task.

**Goal:** Replace the WASM component card gallery with a compact, state-aware registry and safe component-management workflows while preserving dashboard routes and iframe security.

**Architecture:** `WasmPage` owns the single component query and route-level states. `WasmComponentsList` renders a pure, sorted registry from props and owns row-level install/remove interactions. Small exported helpers derive state and ordering from the existing `loaded` and `installed` fields; no backend or generated binding changes are required.

**Tech Stack:** SolidJS, TanStack Solid Query, Solid Router, Kobalte dialogs, Tailwind CSS, Vitest, Solid Testing Library.

---

## Trust boundary

Installed component dashboards are trusted extensions and receive admin-context credentials. Only install trusted components.

## Guardrails

- Work in the existing `feat/admin-ui-refresh` checkout.
- Follow strict red-green-refactor TDD for every behavior change.
- Preserve `/_/admin/wasm` and `/_/admin/wasm/:name`.
- Preserve the current APIs in `src/lib/api/wasm-components.ts`.
- Preserve iframe sandboxing, CSP, path validation, token messaging, and the development-only unsafe toggle.
- Do not add dependencies, backend changes, search, filters, bulk actions, registry editing, or automatic restart controls.
- Keep errors safe and actionable; do not render raw exception strings.

### Task 1: Component state and ordering model

**Files:**
- Modify: `crates/assets/js/admin/src/components/wasm/WasmComponentsList.tsx`
- Create: `crates/assets/js/admin/tests/wasm-workspace.test.ts`

**Step 1: Write failing state tests**

Create `wasm-workspace.test.ts` with a minimal component factory and assertions for the four state combinations:

```ts
expect(wasmComponentStatus(component({ loaded: true, installed: true }))).toMatchObject({
  key: "running",
  label: "Running",
});
expect(wasmComponentStatus(component({ loaded: false, installed: false })).key).toBe("available");
expect(wasmComponentStatus(component({ loaded: false, installed: true })).key).toBe("install-pending");
expect(wasmComponentStatus(component({ loaded: true, installed: false })).key).toBe("removal-pending");
```

Also assert:

- Both pending states have higher display priority than running and available.
- Sorting places pending components first, then running, then available.
- Components within a state sort by `display_name ?? name` using stable locale comparison.
- `wasmComponentSource` returns `repo_id` when present and otherwise `path`.

**Step 2: Verify RED**

Run:

```bash
pnpm --dir crates/assets/js/admin exec vitest run tests/wasm-workspace.test.ts
```

Expected: FAIL because the exported helpers do not exist.

**Step 3: Implement the minimal pure helpers**

In `WasmComponentsList.tsx`, export a small status type and helpers:

```ts
export type WasmComponentStatus = {
  key: "running" | "available" | "install-pending" | "removal-pending";
  label: string;
  priority: number;
  variant: "success" | "secondary" | "warning";
};

export function wasmComponentStatus(component: WasmComponent): WasmComponentStatus {
  if (component.loaded && component.installed) {
    return { key: "running", label: "Running", priority: 1, variant: "success" };
  }
  if (!component.loaded && component.installed) {
    return {
      key: "install-pending",
      label: "Install pending restart",
      priority: 0,
      variant: "warning",
    };
  }
  if (component.loaded && !component.installed) {
    return {
      key: "removal-pending",
      label: "Removal pending restart",
      priority: 0,
      variant: "warning",
    };
  }
  return { key: "available", label: "Available", priority: 2, variant: "secondary" };
}
```

Add only the minimal `sortWasmComponents` and `wasmComponentSource` helpers required by the tests. Do not add a generic registry model or configuration layer.

**Step 4: Verify GREEN**

Run the focused test again and expect PASS.

**Step 5: Commit**

```bash
git add crates/assets/js/admin/src/components/wasm/WasmComponentsList.tsx crates/assets/js/admin/tests/wasm-workspace.test.ts
git commit -m "feat(admin): model wasm component states"
```

### Task 2: Query-owned compact registry and complete list states

**Files:**
- Modify: `crates/assets/js/admin/src/components/wasm/WasmPage.tsx`
- Modify: `crates/assets/js/admin/src/components/wasm/WasmComponentsList.tsx`
- Create: `crates/assets/js/admin/tests/wasm-workspace-ui.test.tsx`
- Test: `crates/assets/js/admin/tests/wasm-workspace.test.ts`

**Step 1: Write failing list UI tests**

Mock `useQuery`, `useParams`, and the WASM API following the reactive mock pattern in `accounts-workspace-ui.test.tsx`. Add focused tests proving:

- One query owner renders the list without a second `useQuery` call.
- Header description shows total and running counts.
- Refresh has accessible name and matching title and calls `refetch`.
- The semantic table exposes Component, State, Runtime / Version, Source, and Actions headers.
- Rows render icon/fallback, display name, internal name, description, state badge, runtime/version, and selectable source.
- Pending rows sort before running and available rows.
- A pending component shows a `Restart required` warning.
- A stable registry shows compact CLI guidance.
- Loading retains the workspace header and shows progress in the registry region.
- Query failure retains the header, shows safe copy, and offers Retry.
- Empty data shows a useful empty state and the existing CLI command.
- No raw query error text is rendered.

Use real exported helpers and components where possible; mock only router/query/API boundaries.

**Step 2: Verify RED**

Run:

```bash
pnpm --dir crates/assets/js/admin exec vitest run tests/wasm-workspace-ui.test.tsx
```

Expected: FAIL against the card gallery and duplicate query ownership.

**Step 3: Move query ownership to `WasmPage`**

Keep one `useQuery({ queryKey: ["wasm-components"], queryFn: listWasmComponents })` in `WasmPage`.

For the list route, pass query state into `WasmComponentsList` through narrow props:

```ts
{
  components?: WasmComponent[];
  loading: boolean;
  error: boolean;
  refetch: () => Promise<unknown>;
}
```

Guard query data access when `isError` is true so Solid Query cannot throw before the safe error state renders.

Keep route-level loading/error/missing handling for component detail routes.

**Step 4: Replace cards with the compact registry**

Use existing primitives:

- `Header`
- `Button`
- `Badge`
- `Callout`
- `Table`, `TableHeader`, `TableBody`, `TableRow`, `TableHead`, `TableCell`
- `Spinner`

Render a bordered, overflow-safe table with the five approved columns. Keep Component and State first. Use explicit dashboard actions rather than wrapping the row in a link.

The informational callout should be one compact paragraph plus the selectable CLI command. Render the warning variant whenever `components.some(c => c.loaded !== c.installed)`.

Do not add search, filters, pagination, or a table abstraction.

**Step 5: Verify GREEN**

Run both WASM test files and expect PASS.

**Step 6: Commit**

```bash
git add crates/assets/js/admin/src/components/wasm/WasmPage.tsx crates/assets/js/admin/src/components/wasm/WasmComponentsList.tsx crates/assets/js/admin/tests/wasm-workspace.test.ts crates/assets/js/admin/tests/wasm-workspace-ui.test.tsx
git commit -m "feat(admin): build wasm component registry"
```

### Task 3: Safe install and remove workflows

**Files:**
- Modify: `crates/assets/js/admin/src/components/wasm/WasmComponentsList.tsx`
- Modify: `crates/assets/js/admin/tests/wasm-workspace-ui.test.tsx`

**Step 1: Write failing mutation tests**

Add tests for:

- Available repository component exposes an accessible Install action.
- Installed component exposes an accessible Remove action.
- Components without a repository ID cannot show Install.
- Install confirmation names the component and explains restart behavior.
- Remove confirmation names the component and explains it remains loaded until restart.
- Cancel closes without calling the API.
- Confirm awaits the API and disables duplicate submission while pending.
- Success awaits `refetch` before closing.
- Failure keeps the dialog open and renders generic actionable feedback.
- Raw rejected error messages are not disclosed.
- Remove awaits `uninstallWasmComponent`; regression coverage must fail against the current unawaited call.
- Controls have matching accessible names and titles.

Use deferred promises for progress and duplicate-click assertions.

**Step 2: Verify RED**

Run the UI test file and confirm failures match the unsafe current behavior.

**Step 3: Implement minimal safe dialogs**

Keep separate install and remove dialog components, but share no abstraction unless the final code is genuinely shorter.

Each dialog should:

- Hold `open`, `pending`, and local safe `error` signals.
- Clear stale errors when reopened.
- Block close and duplicate actions while pending.
- Await install/remove and then await `props.refetch()`.
- Close only after both succeed.
- Stay open after failure.
- Use explicit Cancel and action labels.
- Use `type="button"` on every action.

Use safe text such as:

- `Unable to install component. Please try again.`
- `Unable to remove component. Please try again.`

Do not render `${error}` or token/server response contents.

**Step 4: Verify GREEN**

Run both WASM test files and expect PASS.

**Step 5: Commit**

```bash
git add crates/assets/js/admin/src/components/wasm/WasmComponentsList.tsx crates/assets/js/admin/tests/wasm-workspace-ui.test.tsx
git commit -m "fix(admin): harden wasm component actions"
```

### Task 4: Complete and secure dashboard detail states

**Files:**
- Modify: `crates/assets/js/admin/src/components/wasm/WasmPage.tsx`
- Modify: `crates/assets/js/admin/src/components/wasm/WasmComponentDetails.tsx`
- Modify: `crates/assets/js/admin/tests/wasm-workspace-ui.test.tsx`
- Test: `crates/assets/js/admin/tests/wasm-workspace.test.ts`

**Step 1: Write failing detail tests**

Add tests proving:

- Unknown component routes show a component-specific not-found state and a return action.
- Components without `admin_ui_path` show a useful no-dashboard state and return action.
- Detail header shows display name/internal name/version without losing Back navigation.
- Sandboxed dashboard loading is visible without removing the iframe.
- A non-OK dashboard response becomes a safe fetch failure with Retry.
- Raw fetch errors are not shown.
- The iframe keeps `sandbox="allow-scripts allow-modals"`, the existing CSP, and a component-specific title.
- The development sandbox toggle remains controlled and accessible.
- Unsafe absolute `admin_ui_path` values continue to be rejected without sending the admin off-site.

Keep security assertions behavioral; do not weaken tests to accommodate implementation.

**Step 2: Verify RED**

Run both WASM test files and confirm the new tests fail for the expected missing states.

**Step 3: Implement route and dashboard states**

In `WasmPage`, keep safe loading/error handling for detail routes, then render either the found detail or the not-found state.

In `WasmComponentDetails`:

- Preserve `getAdminUiPath` path-only validation.
- Check `response.ok` before reading dashboard HTML.
- Add loading and safe error overlays with Retry.
- Keep the iframe mounted and full-size.
- Keep existing token setup, `postMessage`, sandbox, CSP, and development URL adjustment.
- Make the development toggle controlled with `checked={sandboxed()}` rather than `defaultChecked`.
- Remove the unused, broken `YoloWithExtraStepsIframe`; do not replace it.
- Give the Back action an accessible name and matching title.

Do not broaden iframe permissions or introduce a new message protocol.

**Step 4: Verify GREEN**

Run both WASM test files and expect PASS.

Run focused static checks:

```bash
pnpm --dir crates/assets/js/admin exec prettier -w \
  src/components/wasm/WasmPage.tsx \
  src/components/wasm/WasmComponentsList.tsx \
  src/components/wasm/WasmComponentDetails.tsx \
  tests/wasm-workspace.test.ts \
  tests/wasm-workspace-ui.test.tsx
pnpm --dir crates/assets/js/admin exec tsc --noEmit --skipLibCheck
pnpm --dir crates/assets/js/admin exec eslint \
  src/components/wasm/WasmPage.tsx \
  src/components/wasm/WasmComponentsList.tsx \
  src/components/wasm/WasmComponentDetails.tsx \
  tests/wasm-workspace.test.ts \
  tests/wasm-workspace-ui.test.tsx
```

Expected: no errors and no new warnings in modified files.

**Step 5: Commit**

```bash
git add crates/assets/js/admin/src/components/wasm/WasmPage.tsx crates/assets/js/admin/src/components/wasm/WasmComponentsList.tsx crates/assets/js/admin/src/components/wasm/WasmComponentDetails.tsx crates/assets/js/admin/tests/wasm-workspace.test.ts crates/assets/js/admin/tests/wasm-workspace-ui.test.tsx
git commit -m "feat(admin): complete wasm dashboard states"
```

### Task 5: Full validation and browser acceptance

**Files:**
- Modify only if browser validation reveals a defect; add a failing regression test before every fix.

**Step 1: Run full automated validation**

```bash
pnpm --dir crates/assets/js/admin check:format
pnpm --dir crates/assets/js/admin check
pnpm --dir crates/assets/js/admin build
git diff --check
```

Expected:

- Formatting passes.
- TypeScript passes.
- ESLint has no new warnings; only documented pre-existing unrelated warnings may remain.
- All tests pass.
- Production build passes.
- `git diff --check` is clean.

**Step 2: Browser-test the list workspace**

At `http://localhost:3000/_/admin/wasm`, validate:

- Desktop dark mode
- Desktop light mode
- `390×844`
- Header summary and refresh
- Compact CLI guidance
- All four state presentations where fixtures/mocks permit
- Horizontal table access without body overflow
- Source selection
- Explicit dashboard, Install, and Remove actions
- Confirmation cancellation and focus return
- No console exceptions or unexpected failed requests

Do not perform a live install/remove unless it can be safely reversed. Automated mutation tests are authoritative for success/failure behavior.

**Step 3: Browser-test dashboard details**

Validate both fixture dashboards where available:

- Open dashboard
- Back navigation
- Full-size iframe
- Development sandbox toggle
- Mobile header and iframe sizing
- Safe missing-component route
- Safe no-dashboard state

**Step 4: Fix browser defects using TDD**

For each defect:

1. Add a failing regression test.
2. Run it and verify RED.
3. Implement the smallest fix.
4. Run it and verify GREEN.
5. Repeat the full validation commands.

**Step 5: Final review and commit**

Request exact-spec review, then code-quality review. Resolve every blocking finding and re-run validation before the final commit.

```bash
git add crates/assets/js/admin/src/components/wasm crates/assets/js/admin/tests/wasm-workspace*.ts*
git commit -m "fix(admin): close wasm browser review gaps"
```

Skip this commit when browser validation produces no changes.
