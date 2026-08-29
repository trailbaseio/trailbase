# Accounts Workspace Refresh Design

## Goal

Turn the Accounts page into a compact, professional account directory that improves identity scanning, filtering, account management, mutation feedback, responsiveness, and accessibility while preserving existing routes, APIs, permissions, and safety behavior.

## Constraints

- Preserve `/_/admin/auth` and the existing account APIs.
- Preserve URL-backed filtering, pagination, and page size.
- Preserve advanced filter-expression support.
- Preserve TanStack Query, TanStack Table, SolidJS, Kobalte, existing forms, sheets, dialogs, and dirty-state protection.
- Preserve CLI-only admin-status changes.
- Preserve account creation, editing, deletion, token minting, sorting, and pagination behavior.
- Add no dependency and make no backend changes.
- Do not introduce a generic workspace controller or speculative account-management abstraction.

## Workspace Structure

The page becomes a compact account directory rather than a raw database table.

The header contains:

- **Accounts** title.
- Total account count.
- Short workspace description.
- Refresh action.
- Primary **Add account** action that remains visible on desktop and mobile.

Below the header, a toolbar contains:

- Default search for email, username, or ID.
- An **Advanced filter** control that reveals the existing filter-expression input.
- Clear/apply behavior.
- URL-backed search/filter state.
- Pagination reset when the query changes.

## Table Model

Use five compact columns:

1. **Account** — preferred email or username, optional secondary username/email, and shortened copyable UUID.
2. **Status** — compact Admin, Verified, or Verification pending badges.
3. **Provider** — Password, known OAuth provider, or numeric provider identifier.
4. **Created** — relative time with exact UTC timestamp in the native title.
5. **Last updated** — relative time with exact UTC timestamp in the native title.

Rows remain dense and clickable. The table avoids tall wrapped timestamps and unnecessary horizontal width. On narrow screens, Account and Status remain the strongest columns while secondary columns can scroll horizontally rather than turning rows into cards.

## Search and Filtering

Default search is optimized for common identity lookup by email, username, or ID. Advanced mode preserves the current filter-expression syntax and backend behavior.

Search/filter values, pagination, and page size remain URL-backed. Applying a changed query resets to page one. Refresh remains a separate action.

## Account Details and Editing

Selecting a row opens one combined details/edit sheet using the existing safe-sheet mechanism.

The sheet contains:

- Identity summary.
- Admin, verification, and provider badges.
- Full copyable account ID.
- Editable email, unverified email, username, and password fields.
- A labeled token-copy action when token minting is allowed.
- A CLI-only admin-status explanation.
- A visually isolated danger section for permanent deletion.

The current delete confirmation remains. Admin status remains read-only in the UI.

## Add Account

The existing account-creation behavior remains in a redesigned sheet with clearer hierarchy, field labels, submission progress, and error feedback. Email, password, verified, and admin values retain their current validation and API contract.

## State and Feedback

The toolbar stays mounted during all states.

- **Loading:** Keep the workspace visible and use the table loading treatment.
- **Error:** Show contained, safe user-facing copy with Retry.
- **Schema empty:** Explain that no accounts exist and offer Add account.
- **Filter empty:** Explain that no accounts match and offer Clear search/filter.
- **Mutation progress:** Disable the initiating action and show progress text.
- **Mutation error:** Keep the sheet open and show contained feedback.
- **Mutation success:** Refresh account data and close when appropriate.
- **Selection cleanup:** Clear selection when the selected account disappears after filtering, refetch, update, or deletion.
- **Token minting:** Report copy success and failures rather than silently swallowing errors.

Deletion closes only after successful completion.

## Architecture

`AccountsPage` continues to own:

- URL state.
- TanStack Query fetching and invalidation.
- Sorting and pagination.
- Selected account.
- Add/edit sheet visibility.

`AddUser` remains a separate form component. Existing account API functions remain unchanged.

Small pure helpers may format:

- Preferred account identity.
- Short UUID display.
- Status metadata.
- Provider labels.
- Relative timestamps.

Reuse existing table, badge, button, callout, sheet, dialog, form, tooltip/title, and toast primitives. Do not add a generic account controller or new dependency.

## Accessibility

- Every icon-only action has an accessible name and title.
- Search, advanced filtering, pagination, and sheet actions are keyboard reachable.
- Status is conveyed with text, not color alone.
- Rows expose clear interactive behavior.
- Mutation and copy feedback is announced through existing toast/live-region behavior.
- Destructive confirmation retains explicit account identity and cancel/delete actions.
- Focus returns predictably when sheets and dialogs close.

## Validation

Automated coverage should include:

- Identity, UUID, status, provider, and relative-time helpers.
- URL-backed search/filter behavior and pagination reset.
- Advanced-filter visibility.
- Compact column content.
- Loading, error, no-account, and no-match states.
- Row selection and selected-account cleanup.
- Add, edit, token, and delete mutation feedback.
- Destructive confirmation safety.
- Responsive action availability and accessible labels.

Browser acceptance should cover:

- Desktop light and dark themes.
- Narrow mobile layout.
- Default search and advanced filters.
- Sorting and pagination.
- Add and edit sheets.
- Token copy.
- Delete confirmation.
- Loading, error, and empty states where practical.
- Keyboard and screen-reader semantics.
