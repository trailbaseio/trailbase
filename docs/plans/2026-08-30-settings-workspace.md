# Settings Workspace Refresh Implementation Plan

> **REQUIRED SUB-SKILL:** Use the executing-plans skill to implement this plan task-by-task.

**Goal:** Refresh all seven Settings categories into a compact, configuration-first workspace with reliable save/reset, mutation, accessibility, and responsive behavior while preserving routes and backend contracts.

**Architecture:** Keep the existing Settings route, nested sidebar, TanStack Query/Form state, protobuf configuration API, and category components. Add one shared sticky form-action component, centralize visible category metadata in `SettingsPage.tsx`, and improve each category in place. Operational mutations stay local and use existing APIs; no new dependency or backend endpoint is introduced.

**Tech Stack:** SolidJS, Solid Router, TanStack Solid Query/Form/Table, Kobalte primitives, Tailwind CSS, Vitest, Testing Library, protobuf-generated config types.

---

## Constraints

- Work directly on `feat/admin-ui-refresh`; do not create a worktree.
- Preserve `/settings/:group?`, category route values, `settings:state`, config hashes, permissions, and all backend APIs.
- Use sequential red-green TDD and commit after each task.
- Never render raw backend errors, log secrets, report success before awaited completion, or close failed mutations.
- Do not add an overview dashboard, global cross-category save, schema editor, import/export UI, dependency, or backend change.

### Task 1: Shared Settings workspace shell

**Files:**
- Create: `crates/assets/js/admin/src/components/settings/SettingsFormActions.tsx`
- Modify: `crates/assets/js/admin/src/components/settings/SettingsPage.tsx`
- Create: `crates/assets/js/admin/tests/settings-workspace-ui.test.tsx`

**Step 1: Write failing workspace tests**

Cover:

- visible labels are General, Email, Authentication, Backups, Jobs, Databases, Schemas
- route values remain host, email, auth, backup, jobs, data, schema
- invalid routes fall back to General
- mobile SidebarTrigger and desktop Settings sidebar remain available
- refresh has an accessible name
- dirty category navigation opens the existing confirmation dialog
- confirming navigation clears dirty state and preserves the destination route
- shared form actions are hidden while clean, visible while dirty, disable during submission, call Reset, and expose Save changes

Use mocked router/query state and a minimal category child rather than loading every settings implementation.

**Step 2: Run the focused test and verify RED**

```bash
cd crates/assets/js/admin
pnpm exec vitest run tests/settings-workspace-ui.test.tsx
```

Expected: FAIL because labels and `SettingsFormActions` behavior do not exist.

**Step 3: Implement the minimum shell**

- Rename visible Host to General and Auth to Authentication without changing routes.
- Constrain content to a readable `max-w` while retaining vertical scrolling.
- Give refresh an accessible label and keep it wired to `invalidateConfig`.
- Replace `markDirty()` with `setDirty(boolean)` in `CommonProps` so reset can clear the parent guard.
- Add `SettingsFormActions` as a sticky, responsive action bar accepting `dirty`, `canSubmit`, `isSubmitting`, and `onReset`; render Reset and Save changes only while dirty.
- Preserve `ConfirmCloseDialog`, sidebar primitives, and `settings:state`.

**Step 4: Run focused tests and verify GREEN**

Run the command from Step 2. Expected: PASS.

**Step 5: Commit**

```bash
git add crates/assets/js/admin/src/components/settings/SettingsFormActions.tsx crates/assets/js/admin/src/components/settings/SettingsPage.tsx crates/assets/js/admin/tests/settings-workspace-ui.test.tsx
git commit -m "feat(admin): refresh settings workspace shell"
```

### Task 2: General settings

**Files:**
- Modify: `crates/assets/js/admin/src/components/settings/SettingsPage.tsx`
- Modify: `crates/assets/js/admin/tests/settings-workspace-ui.test.tsx`

**Step 1: Write failing General tests**

Cover:

- loading, generic error, runtime info, Postgres status, and editable configuration
- Runtime information uses semantic term/value structure and external commit link is safely labeled
- dirty edits reveal sticky Save changes and Reset
- Reset restores initial values and clears dirty state
- failed save retains values and dirty state
- successful save clears dirty state and announces success
- initial loading does not expose save controls

**Step 2: Run focused test and verify RED**

Use the Task 1 command. Expected: FAIL on the new semantics/actions.

**Step 3: Implement General refresh**

- Keep `ServerSettings` in `SettingsPage.tsx`; do not extract speculative modules.
- Present runtime info as a compact `<dl>` and format uptime with the existing native formatter.
- Fix the `Lading...` typo and replace raw errors with generic callouts.
- Wire TanStack form dirty state to `setDirty`.
- Use `form.reset()` for Reset and `SettingsFormActions` for Save changes.
- Keep development exception controls separated and clearly labeled development-only.

**Step 4: Run focused tests and verify GREEN**

**Step 5: Commit**

```bash
git add crates/assets/js/admin/src/components/settings/SettingsPage.tsx crates/assets/js/admin/tests/settings-workspace-ui.test.tsx
git commit -m "feat(admin): refresh general settings"
```

### Task 3: Email settings

**Files:**
- Modify: `crates/assets/js/admin/src/components/settings/EmailSettings.tsx`
- Create: `crates/assets/js/admin/tests/settings-email.test.tsx`

**Step 1: Write failing Email tests**

Cover:

- SMTP, Sender, and Templates hierarchy
- password input remains secret and no secret is rendered in feedback
- sticky Save changes/Reset dirty behavior
- failed configuration save retains edits
- Test Email awaits the request, disables duplicate submission, remains open on failure, shows a generic error, and closes/reports success only after success
- essential template guidance remains visible without opening a tooltip

**Step 2: Run focused test and verify RED**

```bash
pnpm exec vitest run tests/settings-email.test.tsx
```

**Step 3: Implement minimum Email changes**

- Reuse existing accordion and field builders.
- Use `SettingsFormActions` and `form.reset()`.
- Await `adminFetch('/email/test')` with `throwOnError: true`.
- Track only the one pending test-email mutation signal; do not add a mutation abstraction.
- Keep the dialog open with a generic inline error on failure.
- Remove success reporting before completion.

**Step 4: Run focused test and verify GREEN**

**Step 5: Commit**

```bash
git add crates/assets/js/admin/src/components/settings/EmailSettings.tsx crates/assets/js/admin/tests/settings-email.test.tsx
git commit -m "feat(admin): refresh email settings"
```

### Task 4: Authentication settings

**Files:**
- Modify: `crates/assets/js/admin/src/components/settings/AuthSettings.tsx`
- Create: `crates/assets/js/admin/tests/settings-auth.test.tsx`

**Step 1: Write failing Authentication tests**

Cover:

- password policy, OTP, token, OAuth, and public-key sections remain present
- generic provider/config loading failures
- OAuth callback address uses the configured site URL or current origin
- incomplete providers remain excluded from submitted config
- secrets stay password inputs and never reach console output
- dirty Save changes/Reset behavior
- failed save retains edits; success clears dirty state
- provider reset/remove controls have accessible names

**Step 2: Run focused test and verify RED**

```bash
pnpm exec vitest run tests/settings-auth.test.tsx
```

**Step 3: Implement minimum Authentication changes**

- Remove debug logging of submitted OAuth form values/configuration.
- Preserve `configToProxy`/`proxyToConfig` and protobuf deep copies.
- Replace tooltip-only essential policy guidance with visible descriptions where needed; keep optional detail tooltips.
- Add accessible names to provider reset/remove actions.
- Use `SettingsFormActions` and `form.reset()`.
- Replace raw query errors with generic callouts.

**Step 4: Run focused test and verify GREEN**

**Step 5: Commit**

```bash
git add crates/assets/js/admin/src/components/settings/AuthSettings.tsx crates/assets/js/admin/tests/settings-auth.test.tsx
git commit -m "feat(admin): refresh authentication settings"
```

### Task 5: Backups operations

**Files:**
- Modify: `crates/assets/js/admin/src/components/settings/BackupSettings.tsx`
- Create: `crates/assets/js/admin/tests/settings-backups.test.tsx`

**Step 1: Write failing Backups tests**

Cover:

- loading, empty, generic error, and populated table states
- timestamp and rolling-window presentation
- Trigger Backup awaits completion, prevents duplicates, and refreshes only after success
- Delete and Restore require confirmation
- failed Delete/Restore keeps confirmation open and shows a generic error
- successful operations close confirmation, refresh where required, and announce completion
- every icon action has an accessible name

**Step 2: Run focused test and verify RED**

```bash
pnpm exec vitest run tests/settings-backups.test.tsx
```

**Step 3: Implement minimum Backups changes**

- Reuse the existing `Dialog` primitive for one selected backup/action confirmation state.
- Use one pending action signal; disable competing actions while pending.
- Await all APIs and catch errors locally without raw error rendering.
- Keep the compact semantic table and current backup APIs.

**Step 4: Run focused test and verify GREEN**

**Step 5: Commit**

```bash
git add crates/assets/js/admin/src/components/settings/BackupSettings.tsx crates/assets/js/admin/tests/settings-backups.test.tsx
git commit -m "feat(admin): harden backup operations"
```

### Task 6: Jobs configuration and operations

**Files:**
- Modify: `crates/assets/js/admin/src/components/settings/JobSettings.tsx`
- Create: `crates/assets/js/admin/tests/settings-jobs.test.tsx`

**Step 1: Write failing Jobs tests**

Cover:

- loading, empty, generic error, and table states
- cron validation remains enforced
- schedule/enabled changes reveal Save changes/Reset
- reset restores initial job configuration
- successful save refetches jobs and clears dirty state; failure retains edits
- Run now awaits completion, prevents duplicate runs for that action, refreshes afterward, and announces generic failure/success without logging backend details
- table remains horizontally contained on mobile

**Step 2: Run focused test and verify RED**

```bash
pnpm exec vitest run tests/settings-jobs.test.tsx
```

**Step 3: Implement minimum Jobs changes**

- Preserve `buildFormProxy`, `extractConfig`, sorting, and cron semantics.
- Use `SettingsFormActions` and `form.reset()`.
- Track the pending job ID only; avoid a generic mutation manager.
- Remove execution-result console output.
- Add accessible names to Run now and Enabled controls.
- Wrap the table in bounded horizontal overflow.

**Step 4: Run focused test and verify GREEN**

**Step 5: Commit**

```bash
git add crates/assets/js/admin/src/components/settings/JobSettings.tsx crates/assets/js/admin/tests/settings-jobs.test.tsx
git commit -m "feat(admin): refresh jobs settings"
```

### Task 7: Database operations

**Files:**
- Modify: `crates/assets/js/admin/src/components/settings/DatabaseSettings.tsx`
- Create: `crates/assets/js/admin/tests/settings-databases.test.tsx`

**Step 1: Write failing Databases tests**

Cover:

- Postgres unsupported, loading, generic error, empty, and linked database states
- Link requires a valid non-empty native-pattern name
- Link awaits success and keeps the dialog/value on failure
- Unlink requires confirmation
- failed unlink preserves selection; success clears selection and refreshes configuration
- operational actions have accessible names
- import/export remains informational only

**Step 2: Run focused test and verify RED**

```bash
pnpm exec vitest run tests/settings-databases.test.tsx
```

**Step 3: Implement minimum Database changes**

- Preserve existing APIs and name pattern.
- Use native form validation for the Link dialog.
- Call `setConfig` with throwing behavior so callers can distinguish failure.
- Add one confirmation dialog for unlinking the selected names.
- Close/clear only after success; use generic inline failure feedback.
- Keep the existing table helper and Postgres boundary.

**Step 4: Run focused test and verify GREEN**

**Step 5: Commit**

```bash
git add crates/assets/js/admin/src/components/settings/DatabaseSettings.tsx crates/assets/js/admin/tests/settings-databases.test.tsx
git commit -m "feat(admin): harden database settings"
```

### Task 8: Read-only Schemas

**Files:**
- Modify: `crates/assets/js/admin/src/components/settings/SchemaSettings.tsx`
- Create: `crates/assets/js/admin/tests/settings-schemas.test.tsx`

**Step 1: Write failing Schemas tests**

Cover:

- loading, generic error, empty, Postgres, and populated states
- schemas are sorted without mutating query data
- native search filters by name
- built-in badges and schema names remain visible
- malformed schema JSON is rendered safely as source text rather than throwing
- no form or Save changes control is rendered

**Step 2: Run focused test and verify RED**

```bash
pnpm exec vitest run tests/settings-schemas.test.tsx
```

**Step 3: Implement minimum Schemas changes**

- Remove the unused TanStack form and TODO submission path.
- Add one local search signal and filter the sorted copy.
- Parse/pretty-print valid JSON; fall back to raw schema text on malformed input.
- Keep the accordion, built-in badge, example SQL, and Postgres message.

**Step 4: Run focused test and verify GREEN**

**Step 5: Commit**

```bash
git add crates/assets/js/admin/src/components/settings/SchemaSettings.tsx crates/assets/js/admin/tests/settings-schemas.test.tsx
git commit -m "feat(admin): refresh schema settings"
```

### Task 9: Whole-workspace integration and acceptance

**Files:**
- Modify as required by findings only:
  - `crates/assets/js/admin/src/components/settings/SettingsPage.tsx`
  - `crates/assets/js/admin/src/components/settings/SettingsFormActions.tsx`
  - category files and focused tests above
- Update: `docs/plans/2026-08-30-settings-workspace.md` only if implementation evidence needs recording

**Step 1: Run focused Settings tests**

```bash
cd crates/assets/js/admin
pnpm exec vitest run tests/settings-*.test.tsx
```

Expected: all Settings tests pass.

**Step 2: Run full automated gates**

```bash
pnpm check:format
pnpm check
pnpm build
cd /Users/markb/dev/trailbase-ui
git diff --check
```

Expected: zero failures.

**Step 3: Run browser acceptance**

Use the existing backend/Vite session or restart with the process tool. Validate every category at:

- desktop light `1440×900`
- desktop dark `1440×900`
- mobile `390×844`

Verify route/category navigation, mobile sidebar, no horizontal body overflow, representative dirty/save/reset flow, unsaved-navigation confirmation, Email test dialog failure/success, Authentication secret containment, Backup confirmations, Job Run now pending behavior, Database link/unlink validation, Schema search/malformed rendering, keyboard focus, console errors, and expected network requests. Restore `1440×900` afterward.

**Step 4: Run independent reviews**

- Exact-spec review against `docs/plans/2026-08-30-settings-workspace-design.md` and this plan.
- Code-quality/security/accessibility review of the entire Settings diff.
- Resolve findings with focused red-green tests and rerun Steps 1–3.

**Step 5: Commit final corrections**

```bash
git add <only reviewed Settings files>
git commit -m "fix(admin): complete settings workspace refresh"
```

**Step 6: Report evidence and request push confirmation**

Report commits, test counts, build status, browser dimensions/results, review verdicts, residual risks, and current branch status. Do not push without confirmation.
