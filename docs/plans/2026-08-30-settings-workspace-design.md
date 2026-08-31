# Settings Workspace Refresh Design

## Goal

Turn Settings into a professional, configuration-first workspace while preserving existing routes, APIs, protobuf configuration, permissions, storage, and backend behavior.

## Principles

- Keep the current SolidJS, Kobalte, Tailwind, TanStack Query/Form/Table, and shared UI primitives.
- Preserve all routes, including `/settings/host`, while modernizing visible labels.
- Keep configuration changes explicit and operational actions local to their resources.
- Prefer dense, readable forms over a dashboard or nested navigation.
- Add no dependencies, backend changes, schema editor, or global cross-category form.

## Workspace Structure

The Settings sidebar remains the primary category navigator. Visible categories become:

- General (`host`)
- Email (`email`)
- Authentication (`auth`)
- Backups (`backup`)
- Jobs (`jobs`)
- Databases (`data`)
- Schemas (`schema`)

Routes and persisted category selection remain compatible. Desktop keeps the application sidebar and Settings sidebar. Mobile uses the existing off-canvas Settings sidebar as the only category picker.

The content area uses a constrained readable width, compact sections, consistent headings, descriptions, labels, and controls. Long Email and Authentication pages remain vertically scrollable rather than introducing tabs or duplicate navigation.

## Save and Action Semantics

Editable categories receive a sticky action bar with **Save changes** and **Reset**. It appears only while dirty, remains available on long forms, announces progress, and prevents duplicate submission.

Successful saves clear dirty state and invalidate the shared configuration query. Failed saves retain edits and show a generic actionable error. Navigation with unsaved changes preserves the existing confirmation flow.

Immediate operations remain local:

- Backups: trigger, restore, delete
- Jobs: run immediately
- Databases: link and unlink
- Email: send test message
- Authentication: download public key

Consequential or destructive actions use confirmation where appropriate. Pending actions disable duplicate requests and only close dialogs after success.

## Categories

### General

Present runtime information as a compact definition list. Keep application name, site URL, and log retention as the editable configuration section. Separate development-only exception controls from production settings.

### Email

Group SMTP, sender identity, and templates. Keep SMTP and sender fields visible and templates collapsible. Test email remains secondary to Save and reports success only after the request completes.

### Authentication

Keep password policy, OTP, token lifetimes, OAuth providers, and public-key download. Use plain-language labels and persistent descriptions where information is essential. OAuth secrets remain password inputs and never appear in feedback or logs.

### Backups

Use a compact operational table with readable timestamps and explicit actions. Restore and delete require confirmation. Trigger, restore, and delete expose pending states and refresh data only after success.

### Jobs

Use a compact operational table for name, schedule, next run, last run, enabled state, and Run now. Schedule/enabled changes participate in the category save flow. Run now remains immediate and exposes pending state.

### Databases

Preserve SQLite linking and the Postgres unsupported state. Validate link names before mutation. Unlink requires confirmation and only clears selection after success.

### Schemas

Remain read-only. Sort schemas, show built-in badges, support compact searching/collapsing, and format JSON safely. Do not add an editor.

## Data Flow and Errors

Existing TanStack queries and protobuf configuration APIs remain authoritative. Each category continues loading only its current required data. Configuration saves preserve the hash-based concurrency check and invalidate the shared config query after success.

Loading, empty, unsupported, and failure states use shared spinner/callout patterns. Raw backend details are not rendered. Failed forms and dialogs remain open and actionable.

## Accessibility and Responsive Behavior

- Persistent labels, descriptions, validation associations, and visible focus states.
- Essential guidance is not tooltip-only.
- Dirty, saving, success, error, and operation progress use text and live regions.
- Icon actions have explicit accessible names.
- Tables retain semantic headers and mobile overflow containment.
- Desktop content is centered and readable; mobile forms become single-column.
- Sticky actions become full-width on mobile.
- Dialogs remain viewport-contained and restore focus correctly.

## Testing

Sequential TDD covers:

1. Shared workspace shell, category metadata, dirty navigation, and sticky actions.
2. General.
3. Email.
4. Authentication.
5. Backups.
6. Jobs.
7. Databases.
8. Schemas.
9. Whole-workspace accessibility, security, and responsive review.

Automated coverage verifies category routing and persistence, dirty navigation confirmation, reset/save behavior, loading and generic failures, successful and failed mutations, pending protection, confirmations, secret handling, mobile navigation, and semantic labels.

Browser acceptance covers every category at `1440×900` in light and dark themes and at `390×844`, including overflow, keyboard navigation, dirty prompts, save/reset, representative operations, dialog focus, console output, and network requests.

Final gates are TypeScript, ESLint, Prettier, full tests, production build, and `git diff --check`.

## Non-Goals

- Backend or route changes
- New dependencies
- Global cross-category save
- Settings overview dashboard
- Duplicate navigation or nested category tabs
- JSON schema editing
- New backup retention configuration UI
- New database import/export tooling
