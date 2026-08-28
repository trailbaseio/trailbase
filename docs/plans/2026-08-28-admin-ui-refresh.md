# TrailBase Admin UI Refresh Implementation Plan

> **REQUIRED SUB-SKILL:** Use the executing-plans skill to implement this plan task-by-task.

**Goal:** Refresh every TrailBase admin surface into a cohesive, professional developer tool with an expandable labeled sidebar and consistent light and dark themes.

**Architecture:** Keep the existing SolidJS application, routes, API calls, and Kobalte primitives. Establish semantic theme tokens and shared component styling first, then replace the fixed navigation shell and clean page layouts without changing application behavior. Reuse the existing sidebar primitive and installed icon set; add no dependencies.

**Tech Stack:** SolidJS, TypeScript, Tailwind CSS 4, Kobalte, TanStack Query/Form/Table, Nanostores, Vitest, Testing Library, Vite

---

### Task 1: Establish the semantic light and dark themes

**Files:**
- Create: `crates/assets/js/admin/tests/theme.test.ts`
- Modify: `crates/assets/js/admin/src/index.css`
- Modify: `crates/assets/js/admin/src/lib/theme.ts`

**Step 1: Write focused theme tests**

Test the observable theme contract rather than CSS implementation details:

```ts
import { beforeEach, describe, expect, test, vi } from "vitest";
import {
  $themePreference,
  applyResolvedTheme,
  initializeTheme,
} from "@/lib/theme";

beforeEach(() => {
  document.documentElement.className = "";
  document.documentElement.removeAttribute("data-kb-theme");
  $themePreference.set(undefined);
});

describe("theme", () => {
  test("applies the selected theme to the document", () => {
    applyResolvedTheme("dark");
    expect(document.documentElement).toHaveClass("dark");
    expect(document.documentElement).toHaveAttribute("data-kb-theme", "dark");

    applyResolvedTheme("light");
    expect(document.documentElement).not.toHaveClass("dark");
    expect(document.documentElement).toHaveAttribute("data-kb-theme", "light");
  });

  test("uses the system preference when no choice is stored", () => {
    vi.spyOn(window, "matchMedia").mockReturnValue({ matches: true } as MediaQueryList);
    initializeTheme();
    expect(document.documentElement).toHaveClass("dark");
  });
});
```

**Step 2: Run the focused test**

Run: `pnpm --dir crates/assets/js/admin vitest run tests/theme.test.ts`
Expected: existing behavior passes; adjust only test mocks if JSDOM requires full `matchMedia` methods.

**Step 3: Replace the token definitions**

In `src/index.css`, define one complete semantic palette for `:root` and one for `.dark, [data-kb-theme="dark"]`. Include background, foreground, card, popover, muted, border, input, primary, secondary, accent, ring, status, and sidebar roles. Remove the duplicate legacy palette and commented color experiments. Keep TrailBase blue as the primary accent, use slate-blue neutrals, and expose all roles through `@theme`.

Set the body to a system UI font stack, enable antialiasing, use tabular numerals for data-oriented elements, and set consistent selection, scrollbar, focus, and transition styling. Do not fetch a web font.

**Step 4: Keep theme behavior minimal**

Retain the existing persisted `light | dark` preference and system fallback. Simplify comments and ensure the mutation observer reacts to both `class` and `data-kb-theme` changes without introducing a third theme mode.

**Step 5: Verify and commit**

Run: `pnpm --dir crates/assets/js/admin vitest run tests/theme.test.ts && pnpm --dir crates/assets/js/admin check:format`
Expected: PASS.

```bash
git add crates/assets/js/admin/src/index.css crates/assets/js/admin/src/lib/theme.ts crates/assets/js/admin/tests/theme.test.ts
git commit -m "style(admin): establish cohesive light and dark themes"
```

### Task 2: Polish the shared component primitives

**Files:**
- Modify: `crates/assets/js/admin/src/components/ui/button.tsx`
- Modify: `crates/assets/js/admin/src/components/ui/card.tsx`
- Modify: `crates/assets/js/admin/src/components/ui/text-field.tsx`
- Modify: `crates/assets/js/admin/src/components/ui/number-field.tsx`
- Modify: `crates/assets/js/admin/src/components/ui/select.tsx`
- Modify: `crates/assets/js/admin/src/components/ui/checkbox.tsx`
- Modify: `crates/assets/js/admin/src/components/ui/switch.tsx`
- Modify: `crates/assets/js/admin/src/components/ui/table.tsx`
- Modify: `crates/assets/js/admin/src/components/ui/tabs.tsx`
- Modify: `crates/assets/js/admin/src/components/ui/dialog.tsx`
- Modify: `crates/assets/js/admin/src/components/ui/sheet.tsx`
- Modify: `crates/assets/js/admin/src/components/ui/tooltip.tsx`
- Modify: `crates/assets/js/admin/src/components/ui/badge.tsx`
- Modify: `crates/assets/js/admin/src/components/ui/callout.tsx`
- Modify: `crates/assets/js/admin/src/components/ui/toast.tsx`
- Modify: `crates/assets/js/admin/src/components/ui/accordion.tsx`
- Modify: `crates/assets/js/admin/src/components/ui/sidebar.tsx`
- Modify: `crates/assets/js/admin/src/components/ui/skeleton.tsx`
- Modify: `crates/assets/js/admin/src/components/Spinner.tsx`
- Modify: `crates/assets/js/admin/src/components/IconButton.tsx`
- Modify: `crates/assets/js/admin/src/components/FilterBar.tsx`

**Step 1: Normalize control dimensions and interaction states**

Use shared compact defaults: 36px controls, 32px small/icon controls, medium corner radius, visible `focus-visible` rings, restrained hover colors, and no dramatic `active:scale-90`. Preserve every existing variant and public prop.

**Step 2: Normalize surfaces and overlays**

Cards use subtle surface separation rather than prominent shadows. Dialogs, sheets, selects, tooltips, and toasts use the same border, radius, elevated background, and shadow language. Ensure close buttons and destructive variants remain accessible.

**Step 3: Improve dense data primitives**

Reduce table header and cell padding, use a muted header surface, strengthen selected-row state, and preserve horizontal overflow. Align checkboxes, filters, tabs, badges, skeletons, and loading indicators with the semantic tokens.

**Step 4: Format and type-check**

Run: `pnpm --dir crates/assets/js/admin format && pnpm --dir crates/assets/js/admin check`
Expected: TypeScript, ESLint, and all Vitest tests pass.

**Step 5: Commit**

```bash
git add crates/assets/js/admin/src/components/ui crates/assets/js/admin/src/components/{Spinner,IconButton,FilterBar}.tsx
git commit -m "style(admin): unify shared interface components"
```

### Task 3: Replace the fixed icon rail with the responsive application shell

**Files:**
- Create: `crates/assets/js/admin/tests/sidebar.test.tsx`
- Modify: `crates/assets/js/admin/src/App.tsx`
- Modify: `crates/assets/js/admin/src/components/Navbar.tsx`
- Modify: `crates/assets/js/admin/src/components/ui/sidebar.tsx`
- Modify: `crates/assets/js/admin/src/components/Version.tsx`

**Step 1: Write the sidebar behavior test**

Render `SidebarProvider`, `Sidebar`, `SidebarInset`, and `SidebarTrigger`. Assert that activating the trigger changes the desktop sidebar from `data-state="expanded"` to `data-state="collapsed"` and writes the persisted state. Stub `window.matchMedia` to report desktop.

**Step 2: Run the test and confirm the persistence gap**

Run: `pnpm --dir crates/assets/js/admin vitest run tests/sidebar.test.tsx`
Expected: FAIL until the provider reads its stored state on initialization.

**Step 3: Implement the minimal persistence fix**

Keep persistence inside `ui/sidebar.tsx`, where every sidebar caller benefits. Initialize from the existing `sidebar:state` cookie when available, retain controlled usage, and set widths to 15rem expanded and 4rem collapsed. Do not add another store.

**Step 4: Build the application shell**

In `Navbar.tsx`, replace separate vertical and horizontal implementations with one sidebar content tree using the existing sidebar primitives. Group navigation as:

- Data: Tables, SQL Editor, ERD
- Operate: Accounts, WASM, Logs, OpenAPI
- System: Settings

Include logo/product name, active route styling, tooltips in collapsed mode, collapse trigger, theme toggle, version, and account control. Preserve the unsaved-change dialog behavior.

In `App.tsx`, wrap authenticated routes in `SidebarProvider`; render the application sidebar and `SidebarInset`. Add a compact mobile header containing the menu trigger and current TrailBase identity. Remove fixed 58px/48px positioning and duplicate mobile detection.

**Step 5: Verify behavior**

Run: `pnpm --dir crates/assets/js/admin vitest run tests/sidebar.test.tsx && pnpm --dir crates/assets/js/admin check`
Expected: PASS; keyboard shortcut and mobile drawer remain functional.

**Step 6: Commit**

```bash
git add crates/assets/js/admin/src/App.tsx crates/assets/js/admin/src/components/{Navbar,Version}.tsx crates/assets/js/admin/src/components/ui/sidebar.tsx crates/assets/js/admin/tests/sidebar.test.tsx
git commit -m "feat(admin): add responsive labeled application sidebar"
```

### Task 4: Standardize page chrome and refresh dashboard and authentication

**Files:**
- Modify: `crates/assets/js/admin/src/components/Header.tsx`
- Modify: `crates/assets/js/admin/src/components/IndexPage.tsx`
- Modify: `crates/assets/js/admin/src/components/auth/LoginPage.tsx`
- Modify: `crates/assets/js/admin/src/components/auth/Profile.tsx`
- Modify: `crates/assets/js/admin/src/components/auth/AuthButton.tsx`
- Modify: `crates/assets/js/admin/src/components/ErrorBoundary.tsx`

**Step 1: Extend the existing Header rather than add another abstraction**

Add an optional description and make the existing left/right/leading slots responsive. Use consistent page padding, title hierarchy, and border treatment. Keep all current call sites valid.

**Step 2: Recompose the dashboard**

Remove `ColorPalette`. Keep the existing SQL-backed metrics and navigation data, but present them as a compact overview with system facts, quick links, clear loading/error/empty states, and corrected WASM route. Use semantic colors only.

**Step 3: Recompose authentication**

Add the TrailBase identity and supporting copy, constrain the form width, improve spacing and non-admin messaging, and keep all password, OTP, MFA, and logout behavior unchanged.

**Step 4: Verify**

Run: `pnpm --dir crates/assets/js/admin check`
Expected: PASS.

**Step 5: Commit**

```bash
git add crates/assets/js/admin/src/components/{Header,IndexPage,ErrorBoundary}.tsx crates/assets/js/admin/src/components/auth
git commit -m "style(admin): refresh dashboard and authentication"
```

### Task 5: Refresh data-management pages

**Files:**
- Modify: `crates/assets/js/admin/src/components/Table.tsx`
- Modify: `crates/assets/js/admin/src/components/accounts/AccountsPage.tsx`
- Modify: `crates/assets/js/admin/src/components/accounts/AddUser.tsx`
- Modify: `crates/assets/js/admin/src/components/tables/TablesPage.tsx`
- Modify: `crates/assets/js/admin/src/components/tables/TablePane.tsx`
- Modify: `crates/assets/js/admin/src/components/tables/CreateAlterTable.tsx`
- Modify: `crates/assets/js/admin/src/components/tables/CreateAlterColumnForm.tsx`
- Modify: `crates/assets/js/admin/src/components/tables/CreateAlterIndex.tsx`
- Modify: `crates/assets/js/admin/src/components/tables/InsertUpdateRow.tsx`
- Modify: `crates/assets/js/admin/src/components/tables/RecordApiSettings.tsx`
- Modify: `crates/assets/js/admin/src/components/tables/Files.tsx`
- Modify: `crates/assets/js/admin/src/components/tables/SchemaDownload.tsx`

**Step 1: Clean the reusable TanStack table presentation**

Apply compact header, row, pagination, pinned-column, empty, selected, and hover styling in `components/Table.tsx`. Preserve sorting, resizing, pinning, and pagination logic.

**Step 2: Align table and account split views**

Use the shared page header, consistent sidebar groups, clearer item selection, compact toolbars, and deliberate primary/destructive actions. Remove hard-coded colors such as `text-white` where semantic foreground tokens exist.

**Step 3: Align sheets and forms**

Standardize section spacing, labels, descriptions, footers, and destructive areas across create/alter/edit sheets. Do not change validation or mutation behavior.

**Step 4: Verify**

Run: `pnpm --dir crates/assets/js/admin check && pnpm --dir crates/assets/js/admin build`
Expected: PASS and production assets generated.

**Step 5: Commit**

```bash
git add crates/assets/js/admin/src/components/Table.tsx crates/assets/js/admin/src/components/accounts crates/assets/js/admin/src/components/tables
git commit -m "style(admin): polish data management workflows"
```

### Task 6: Refresh specialized full-workspace tools

**Files:**
- Modify: `crates/assets/js/admin/src/components/editor/EditorPage.tsx`
- Modify: `crates/assets/js/admin/src/components/erd/ErdPage.tsx`
- Modify: `crates/assets/js/admin/src/components/erd/ErdGraph.tsx`
- Modify: `crates/assets/js/admin/src/components/logs/LogsPage.tsx`
- Modify: `crates/assets/js/admin/src/components/openapi/OpenApiPage.tsx`

**Step 1: Standardize workspace headers and toolbars**

Apply the shared page header and compact controls without constraining editor, graph, map, chart, table, or OpenAPI work areas.

**Step 2: Make embedded visuals theme-aware**

Use semantic CSS variables when configuring CodeMirror, X6, Chart.js, MapLibre overlays, and RapiDoc where their APIs permit. Remove hard-coded grays and ensure text, grid lines, selections, and surfaces remain readable in both modes. Do not rewrite integrations.

**Step 3: Verify**

Run: `pnpm --dir crates/assets/js/admin check && pnpm --dir crates/assets/js/admin build`
Expected: PASS.

**Step 4: Commit**

```bash
git add crates/assets/js/admin/src/components/{editor,erd,logs,openapi}
git commit -m "style(admin): unify full-workspace tools"
```

### Task 7: Refresh WASM and settings workflows

**Files:**
- Modify: `crates/assets/js/admin/src/components/wasm/WasmPage.tsx`
- Modify: `crates/assets/js/admin/src/components/wasm/WasmComponentsList.tsx`
- Modify: `crates/assets/js/admin/src/components/wasm/WasmComponentDetails.tsx`
- Modify: `crates/assets/js/admin/src/components/settings/SettingsPage.tsx`
- Modify: `crates/assets/js/admin/src/components/settings/AuthSettings.tsx`
- Modify: `crates/assets/js/admin/src/components/settings/BackupSettings.tsx`
- Modify: `crates/assets/js/admin/src/components/settings/DatabaseSettings.tsx`
- Modify: `crates/assets/js/admin/src/components/settings/EmailSettings.tsx`
- Modify: `crates/assets/js/admin/src/components/settings/JobSettings.tsx`
- Modify: `crates/assets/js/admin/src/components/settings/SchemaSettings.tsx`
- Modify: `crates/assets/js/admin/src/components/FormFields.tsx`
- Modify: `crates/assets/js/admin/src/components/DestructiveActionButton.tsx`
- Modify: `crates/assets/js/admin/src/components/SafeSheet.tsx`

**Step 1: Standardize sidebar-detail layouts**

Use matching navigation widths, selected states, mobile triggers, content padding, and section hierarchy for WASM and settings.

**Step 2: Standardize settings forms**

Align labels, descriptions, optional-field controls, cards, save bars, loading states, and dirty-state dialogs. Preserve every validator, mutation, and unsaved-change safeguard.

**Step 3: Isolate destructive actions**

Use semantic destructive colors and clear spacing without weakening confirmations.

**Step 4: Verify**

Run: `pnpm --dir crates/assets/js/admin check && pnpm --dir crates/assets/js/admin build`
Expected: PASS.

**Step 5: Commit**

```bash
git add crates/assets/js/admin/src/components/{wasm,settings} crates/assets/js/admin/src/components/{FormFields,DestructiveActionButton,SafeSheet}.tsx
git commit -m "style(admin): polish configuration workflows"
```

### Task 8: Perform system-wide visual and accessibility verification

**Files:**
- Modify as needed: `crates/assets/js/admin/src/**`
- Modify as needed: `crates/assets/js/admin/tests/**`

**Step 1: Search for inconsistent styling**

Run:

```bash
rg -n 'text-white|bg-white|text-black|bg-black|text-gray|bg-gray|border-gray' crates/assets/js/admin/src
```

Expected: only intentional third-party or visualization values remain; replace page chrome occurrences with semantic tokens.

**Step 2: Run the complete automated suite**

Run:

```bash
pnpm --dir crates/assets/js/admin format
pnpm --dir crates/assets/js/admin check
pnpm --dir crates/assets/js/admin build
```

Expected: all commands exit 0.

**Step 3: Start the real application for browser review**

Terminal 1:

```bash
cargo run -- --data-dir client/testfixture run --dev
```

Terminal 2:

```bash
pnpm --dir crates/assets/js/admin dev
```

Review dashboard, tables, accounts, editor, ERD, logs, OpenAPI, WASM, settings, and authentication at desktop and mobile widths. Check both light and dark modes, sidebar expand/collapse persistence, keyboard focus, overflow, dialogs, sheets, loading/empty/error states, and destructive confirmations.

**Step 4: Record representative screenshots**

Capture desktop and mobile screenshots for light and dark dashboard/table/settings views as local review artifacts. Do not commit generated screenshots unless requested.

**Step 5: Commit final fixes**

```bash
git add crates/assets/js/admin
git commit -m "fix(admin): finish responsive theme polish"
```

**Step 6: Final branch verification**

Run: `git status --short && git log --oneline origin/main..HEAD`
Expected: clean working tree and a focused series of admin UI commits ready to push and open as a PR.
