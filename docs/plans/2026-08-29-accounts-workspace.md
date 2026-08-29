# Accounts Workspace Refresh Implementation Plan

> **REQUIRED SUB-SKILL:** Use the executing-plans skill to implement this plan task-by-task.

**Goal:** Replace the oversized raw Accounts table with a compact, search-first account directory and safer, clearer account-management sheets while preserving every existing API and backend behavior.

**Architecture:** `AccountsPage` continues to own URL state, TanStack Query, sorting, pagination, selected-account state, and sheets. Small exported helpers keep identity, status, provider, relative-time, and search-parameter behavior testable; existing shared table, filter, badge, callout, sheet, dialog, form, and toast primitives provide the UI.

**Tech Stack:** SolidJS, TypeScript, TanStack Solid Query/Form/Table, Kobalte, Tailwind CSS, Vitest, Solid Testing Library.

---

## Constraints

- Preserve `/_/admin/auth`.
- Preserve `fetchUsers`, `createUser`, `updateUser`, `deleteUser`, and `mintTokens` backend contracts.
- Preserve raw advanced filter expressions, sorting, pagination, page-size URL state, dirty-sheet protection, and CLI-only admin changes.
- Add no dependency and make no backend change.
- Keep one compact directory table; do not add bulk actions, account cards, a persistent inspector, or a generic workspace controller.
- Use TDD for every non-trivial behavior.

### Task 1: Account presentation helpers and compact columns

**Files:**
- Create: `crates/assets/js/admin/tests/accounts-workspace.test.ts`
- Modify: `crates/assets/js/admin/src/components/accounts/AccountsPage.tsx`

**Step 1: Write failing helper tests**

Create `accounts-workspace.test.ts` and import the following intended exports from `AccountsPage.tsx`:

```ts
import {
  accountIdentity,
  accountStatuses,
  accountProviderLabel,
  formatAccountTime,
  shortAccountId,
} from "@/components/accounts/AccountsPage";
```

Cover these cases with a minimal `UserJson` fixture:

```ts
expect(accountIdentity(user)).toEqual({
  primary: "ada@example.com",
  secondary: "ada",
});
expect(accountIdentity({ ...user, email: null, username: null })).toEqual({
  primary: "Unnamed account",
  secondary: undefined,
});
expect(shortAccountId("8c29281a-be84-44d7-9bb6-00fe8032dfa1"))
  .toBe("8c29281a…dfa1");
expect(accountStatuses({ ...user, admin: true })).toEqual([
  { label: "Admin", variant: "default" },
  { label: "Verified", variant: "success" },
]);
expect(accountStatuses({ ...user, unverified_email: "next@example.com" }))
  .toContainEqual({ label: "Pending verification", variant: "warning" });
expect(accountProviderLabel(0n)).toBe("Password");
expect(accountProviderLabel(12n)).toBe("Google");
expect(accountProviderLabel(99n)).toBe("OAuth 99");
expect(formatAccountTime(1_700_000_000n, 1_700_003_600_000, "en"))
  .toBe("1 hour ago");
```

**Step 2: Run the test and verify failure**

Run:

```bash
pnpm --dir crates/assets/js/admin exec vitest run tests/accounts-workspace.test.ts
```

Expected: FAIL because the helper exports do not exist.

**Step 3: Implement minimal pure helpers**

In `AccountsPage.tsx`:

- Prefer verified email, then username, then unverified email, then `Unnamed account`.
- Only show a distinct secondary identity value.
- Shorten IDs to the first eight and last four characters.
- Return text-bearing status descriptors for Admin plus Verified/Pending verification.
- Use `OAuthProviderId` for known provider labels; use `Password` for `0`; fall back to `OAuth N`.
- Use native `Intl.RelativeTimeFormat` with deterministic `nowMs` and optional locale arguments. Choose the largest useful unit among minutes, hours, days, months, and years; return `just now` for less than one minute.

Keep the helpers in `AccountsPage.tsx`; do not create a utility module for one screen.

**Step 4: Replace `buildColumns` with the approved five-column model**

Create compact columns:

- `account`: primary identity, optional secondary identity, and a copyable shortened ID.
- `status`: one or two text badges.
- `provider`: icon when known plus the provider label.
- `created`: relative time with exact `toUTCString()` in `title`.
- `updated`: relative time with exact `toUTCString()` in `title`.

Use accessible labels/titles on copy and provider imagery. Remove the old raw ID, username, email, admin, OAuth, updated, and created column definitions.

**Step 5: Run focused checks**

Run:

```bash
pnpm --dir crates/assets/js/admin exec vitest run tests/accounts-workspace.test.ts
pnpm --dir crates/assets/js/admin exec tsc --noEmit --skipLibCheck
pnpm --dir crates/assets/js/admin exec eslint src/components/accounts/AccountsPage.tsx tests/accounts-workspace.test.ts
```

Expected: helpers pass, TypeScript passes, no new lint errors.

**Step 6: Commit**

```bash
git add crates/assets/js/admin/src/components/accounts/AccountsPage.tsx crates/assets/js/admin/tests/accounts-workspace.test.ts
git commit -m "refactor(admin): compact account presentation"
```

### Task 2: Simple search and advanced filter toolbar

**Files:**
- Modify: `crates/assets/js/admin/src/lib/api/user.ts`
- Modify: `crates/assets/js/admin/src/components/FilterBar.tsx`
- Modify: `crates/assets/js/admin/src/components/accounts/AccountsPage.tsx`
- Modify: `crates/assets/js/admin/tests/accounts-workspace.test.ts`
- Create: `crates/assets/js/admin/tests/accounts-workspace-ui.test.tsx`
- Modify: `crates/assets/js/admin/tests/filter-bar.test.tsx`

**Step 1: Write failing search-parameter tests**

Add tests for an intended exported helper from `user.ts`:

```ts
import { appendAccountSearchParams } from "@/lib/api/user";

const params = new URLSearchParams("limit=20");
appendAccountSearchParams(params, " ada@example.com ");
expect([...params.entries()]).toEqual([
  ["limit", "20"],
  ["filter[$or][0][email][$like]", "%ada@example.com%"],
  ["filter[$or][1][username][$like]", "%ada@example.com%"],
]);
```

Add a UUID case that also appends:

```ts
["filter[$or][2][id][$eq]", "8c29281a-be84-44d7-9bb6-00fe8032dfa1"]
```

Add empty/whitespace coverage proving no params are appended. Direct `URLSearchParams` construction avoids interpolating user input into the advanced expression parser.

**Step 2: Run and verify failure**

```bash
pnpm --dir crates/assets/js/admin exec vitest run tests/accounts-workspace.test.ts
```

Expected: FAIL because `appendAccountSearchParams` is missing.

**Step 3: Implement safe simple-search params**

In `user.ts`, add `appendAccountSearchParams(params, search)`. Trim input; append email and username `$like` params; append exact ID only for a canonical UUID. Extend `fetchUsers` with an optional final `search?: string` argument and call the helper after `buildListSearchParams`.

Do not change the backend route or advanced-filter parsing.

**Step 4: Generalize the shared filter’s accessible label**

Add an optional `label?: string` prop to `FilterBar`, defaulting to `Filter rows`, and use it for the input `aria-label`. Add a test proving a custom label is rendered without changing current defaults.

**Step 5: Write failing toolbar UI tests**

Export a small `AccountToolbar` from `AccountsPage.tsx`. In `accounts-workspace-ui.test.tsx`, render it with controlled props and test:

- Simple mode shows `Search accounts` and hides `Advanced account filter`.
- The advanced-filter button exposes the raw expression field.
- Applying a simple search calls `onSearch` with the text.
- Applying advanced filter calls `onFilter` with the expression.
- Clear actions submit an empty value.
- Refresh and Add account remain independently labeled actions.

**Step 6: Implement the toolbar and URL state**

Extend `useSearchParams` with:

```ts
search?: string;
filter?: string;
advanced?: string;
pageSize?: string;
pageIndex?: string;
```

Rules:

- `advanced === "true"` selects advanced mode.
- Simple mode fetches with `search` and no advanced filter.
- Advanced mode fetches with `filter` and no simple search.
- Applying either resets `pageIndex` to `0`.
- Switching modes preserves each mode’s URL value but changes the active mode.
- Refresh invalidates `users` without changing URL state.

Use `FilterBar` for both modes with the appropriate label and placeholder. Keep raw syntax help only in advanced mode.

**Step 7: Run focused checks**

```bash
pnpm --dir crates/assets/js/admin exec vitest run tests/accounts-workspace.test.ts tests/accounts-workspace-ui.test.tsx tests/filter-bar.test.tsx
pnpm --dir crates/assets/js/admin exec tsc --noEmit --skipLibCheck
pnpm --dir crates/assets/js/admin exec eslint src/components/accounts/AccountsPage.tsx src/lib/api/user.ts src/components/FilterBar.tsx tests/accounts-workspace*.ts* tests/filter-bar.test.tsx
```

Expected: all focused tests pass and no new lint errors.

**Step 8: Commit**

```bash
git add crates/assets/js/admin/src/lib/api/user.ts crates/assets/js/admin/src/components/FilterBar.tsx crates/assets/js/admin/src/components/accounts/AccountsPage.tsx crates/assets/js/admin/tests/accounts-workspace.test.ts crates/assets/js/admin/tests/accounts-workspace-ui.test.tsx crates/assets/js/admin/tests/filter-bar.test.tsx
git commit -m "feat(admin): add account search toolbar"
```

### Task 3: Directory page hierarchy and complete states

**Files:**
- Modify: `crates/assets/js/admin/src/components/accounts/AccountsPage.tsx`
- Modify: `crates/assets/js/admin/tests/accounts-workspace-ui.test.tsx`

**Step 1: Add failing page-state tests**

Mock `useQuery`, `useQueryClient`, router search params, and the account APIs following the existing ERD page-state test style. Cover:

- Header contains `Accounts`, total count, description, refresh, and Add account.
- Loading keeps the toolbar and Add account mounted.
- Error renders safe `Unable to load accounts` copy and Retry; it does not render the raw thrown error.
- No-account state offers Add account.
- No-match state offers Clear search/filter.
- The table receives dense rows and bottom pagination.
- Row activation opens the selected account sheet.
- Selected account clears if refreshed data no longer contains its ID.

**Step 2: Run and verify failure**

```bash
pnpm --dir crates/assets/js/admin exec vitest run tests/accounts-workspace-ui.test.tsx
```

Expected: new page-state expectations fail.

**Step 3: Implement workspace hierarchy**

Refactor the page root to `flex h-full min-h-0 flex-col`.

Use `Header` with:

- Description based on `total_row_count` when loaded, otherwise `Manage authentication identities and access`.
- Refresh icon button with label/title.
- Primary Add account trigger in `right`.

Place `AccountToolbar` below the header. Keep it mounted during query states.

Render the directory in `min-h-0 flex-1 overflow-auto p-4` with:

```tsx
<Table
  table={accountsTable()}
  loading={users.isLoading}
  onRowClick={...}
  dense
  paginationPosition="bottom"
  emptyState={...}
/>
```

Use a contained error `Callout` with Retry. Distinguish no accounts from no matches. Do not render raw exception strings.

Move the Add account trigger out of `Suspense` and below-the-table placement so it remains visible.

**Step 4: Clear stale selection reactively**

Use a small `createEffect` that only clears `editUser` when query data is available and the selected ID is absent. Do not auto-select another row.

**Step 5: Run focused checks**

```bash
pnpm --dir crates/assets/js/admin exec vitest run tests/accounts-workspace.test.ts tests/accounts-workspace-ui.test.tsx
pnpm --dir crates/assets/js/admin exec tsc --noEmit --skipLibCheck
pnpm --dir crates/assets/js/admin exec eslint src/components/accounts/AccountsPage.tsx tests/accounts-workspace*.ts*
```

Expected: page-state tests pass, TypeScript passes, no new lint errors.

**Step 6: Commit**

```bash
git add crates/assets/js/admin/src/components/accounts/AccountsPage.tsx crates/assets/js/admin/tests/accounts-workspace-ui.test.tsx
git commit -m "feat(admin): build account directory workspace"
```

### Task 4: Account add/edit sheets and mutation safety

**Files:**
- Modify: `crates/assets/js/admin/src/components/accounts/AddUser.tsx`
- Modify: `crates/assets/js/admin/src/components/accounts/AccountsPage.tsx`
- Modify: `crates/assets/js/admin/tests/accounts-workspace-ui.test.tsx`

**Step 1: Write failing sheet and mutation tests**

Add focused UI tests with mocked API functions for:

- Add sheet title, description, fields, and Add account submit label.
- Add/update failures keep the sheet open and show safe contained feedback.
- Add/update buttons disable and show progress while submitting.
- Edit sheet renders identity summary, status/provider badges, full copyable ID, and CLI-only admin note.
- Form dirty state calls `markDirty`.
- Token action has a text label, is absent for admin/pending accounts, reports successful copy, and reports failure.
- Delete is isolated under `Danger zone`.
- Delete confirmation names the account, Cancel does not delete, and Delete only closes/refetches after successful API completion.
- Delete failure keeps the sheet open and reports an error.

**Step 2: Run and verify failure**

```bash
pnpm --dir crates/assets/js/admin exec vitest run tests/accounts-workspace-ui.test.tsx
```

Expected: sheet hierarchy and mutation feedback tests fail.

**Step 3: Refactor `AddUser` minimally**

Add:

- Sheet description.
- Clear section spacing and responsive labels.
- Local safe error signal.
- `Creating…` progress label.
- `form.useStore` dirty tracking calling `props.markDirty()`.
- Close and refetch only after success.
- Safe error callout on failure.

Preserve request values and existing validators.

**Step 4: Refactor edit content**

In `EditSheetContent`:

- Add account identity/status/provider summary.
- Make the full ID copyable with an accessible label.
- Keep the existing editable fields and API request shape.
- Add `form.useStore` dirty tracking.
- Keep the sheet open on update failure.
- Label token action `Copy login tokens`; catch and report mint/copy failure.
- Replace `Submit` with `Save changes` and `Saving…`.
- Keep CLI-only admin callout.

**Step 5: Fix deletion semantics**

Change `DeleteUserButton` so it:

- Uses `Cancel` and `Delete account` labels.
- Tracks deletion progress.
- Awaits `deleteUser`.
- Calls `onDelete` only after success.
- Keeps the confirmation/sheet open and reports safe failure feedback on error.

Do not preserve the current `finally { props.onDelete() }` behavior because it closes/refetches even when deletion fails.

**Step 6: Run focused and full checks**

```bash
pnpm --dir crates/assets/js/admin exec vitest run tests/accounts-workspace.test.ts tests/accounts-workspace-ui.test.tsx
pnpm --dir crates/assets/js/admin check:format
pnpm --dir crates/assets/js/admin check
pnpm --dir crates/assets/js/admin build
git diff --check
```

Expected: all tests pass, build succeeds, no formatting or diff errors. Existing unrelated warnings may remain but no new warning is acceptable.

**Step 7: Commit**

```bash
git add crates/assets/js/admin/src/components/accounts/AddUser.tsx crates/assets/js/admin/src/components/accounts/AccountsPage.tsx crates/assets/js/admin/tests/accounts-workspace-ui.test.tsx
git commit -m "feat(admin): improve account management flows"
```

### Task 5: Browser acceptance and final review

**Files:**
- Modify only files implicated by acceptance defects.
- Add regression tests beside the failing Accounts behavior before each fix.

**Step 1: Start from a clean test fixture**

Use:

- URL: `http://localhost:3000/_/admin/auth`
- Login: `admin@localhost` / `secret`
- Backend: `http://localhost:4000`

Confirm the workspace shows fixture accounts and no new console errors.

**Step 2: Desktop acceptance**

Verify:

- Header count, description, refresh, and Add account hierarchy.
- Compact five-column table with no wrapped timestamp rows.
- Simple email/username/UUID search.
- Advanced expression filter and mode switching.
- Sorting, pagination, and page-size URL state.
- Copy ID feedback.
- Account sheet identity/status/provider details.
- Edit success and error behavior.
- Token-copy eligibility and feedback.
- Delete confirmation and failure-safe semantics.
- No-account/no-match recovery where practical.

**Step 3: Theme and mobile acceptance**

Verify light and dark themes, then emulate `390x844`:

- Search and Add account stay immediately accessible.
- Header actions wrap without overlap.
- Account and Status remain readable.
- Secondary columns scroll horizontally without tall cards.
- Sheets use the full narrow viewport without clipped actions.

**Step 4: Keyboard and accessibility acceptance**

Verify:

- `/` focuses the active search/filter input.
- Tab order reaches mode toggle, refresh, Add account, pagination, rows, and sheet actions.
- Enter/Space opens a focused row.
- Icon actions have accessible names and titles.
- Status text is present without relying on color.
- Focus returns after sheets/dialogs close.

**Step 5: Correct defects with TDD**

For each defect, add the smallest failing regression test, implement the minimum fix, and rerun focused checks.

**Step 6: Final automated verification**

```bash
pnpm --dir crates/assets/js/admin check:format
pnpm --dir crates/assets/js/admin check
pnpm --dir crates/assets/js/admin build
git diff --check
git status --short
```

Expected: formatting, TypeScript, ESLint, all tests, production build, and diff checks pass; only intentional Accounts files remain modified or the tree is clean after task commits.

**Step 7: Request independent reviews**

Run an exact-spec review against this plan and `2026-08-29-accounts-workspace-design.md`, then a code-quality review. Address every actionable finding with a regression test before declaring completion.
