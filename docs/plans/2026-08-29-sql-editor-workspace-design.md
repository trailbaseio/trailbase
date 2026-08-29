# SQL Editor Workspace Design

**Date:** 2026-08-29  
**Status:** Approved

## Goal

Turn the SQL Editor into a professional, execution-focused developer workspace while preserving saved scripts, local persistence, dirty-state protection, schema autocomplete, attached databases, keyboard shortcuts, query execution, cached results, and safety messaging.

## Constraints

- Keep SolidJS, Kobalte, CodeMirror, TanStack Table, Tailwind, existing APIs, routes, persistence keys, and backend behavior.
- Add no dependencies or backend changes.
- Do not add resizable panels, SQL parsing, query history, collaboration, or server-side saved queries.
- Preserve arbitrary SQL execution behavior and the warning to use `LIMIT` for large results.

## Workspace Structure

### Saved-query explorer

Use a collapsible secondary sidebar matching the Tables explorer:

- New query action
- Search field
- Query count
- Compact saved-query rows
- Selected and dirty indicators
- Rename and Delete in a row overflow menu
- Explicit empty and no-search-results states

Persist its state independently under `sql-explorer:state` so it cannot affect the main navigation or other nested sidebars.

### Query header and editor

Use a sticky resource header containing:

- Directional saved-query explorer toggle
- `SQL Editor › query name` context
- Unsaved indicator
- Attached-database selector
- Help access

Render the migration warning as a compact dismissible banner. Keep CodeMirror as the primary surface with schema autocomplete and a restrained minimum height. Place Save as a secondary action and Execute as the primary action. Preserve `Ctrl/Cmd+S` and `Ctrl/Cmd+Enter`.

### Results workspace

Keep results directly below the editor and use the remaining vertical space. The results header shows:

- Success, Error, No rows, or Cached result status
- Returned-row count
- Execution timestamp
- Copy CSV action

Use the same dense grid language as Tables. Add dedicated error and no-data states. Use client-side pagination for rendering large returned datasets while retaining the warning that the complete backend response is still returned.

## Interaction and Feedback

Saved queries remain browser-local. Creating a query selects it immediately. Switching away from a dirty query retains the existing save/discard guard. Rename and Delete move into an overflow menu; deletion receives a lightweight confirmation.

Search filters queries by name without changing the current selection. Empty states provide a direct recovery action.

During execution:

- Disable Execute and show progress.
- Prevent duplicate submissions.
- Keep the previous result visible but mark it stale.
- Update result data and metadata on success.
- Show failures inline and retain the existing error toast.
- Switch mobile users to Results after execution.

Save writes to local storage, clears the dirty state, confirms success, and preserves editor focus. Attached-database selection and current autocomplete behavior remain unchanged.

## Responsive Behavior

- **Desktop:** fixed-width saved-query explorer; vertically stacked editor and results remain simultaneously visible.
- **Tablet:** explorer can fully collapse; workspace uses the reclaimed width.
- **Mobile:** explorer opens as a sheet; Editor and Results become tabs. Executing switches to Results without resetting query state.
- No resizable panels.

## Accessibility

Provide explicit accessible labels for explorer toggling, query creation/rename/deletion, Save, Execute, Copy CSV, and attached-database selection. Menus and dialogs remain keyboard-operable. Selected, dirty, and result states must not rely on color alone. Errors remain selectable and copyable, with predictable focus restoration after dialogs.

## Validation

Automated coverage should include:

- Query filtering and empty states
- Selection and dirty-change protection
- Explorer state isolation
- Execution loading, success, error, no-data, and cached-result states
- CSV generation and escaping
- Result pagination
- Mobile Editor/Results switching
- Accessible names and keyboard behavior

Manual verification should cover desktop, collapsed desktop, tablet, and mobile layouts; light and dark themes; query creation, rename, deletion, save, execution, database attachment, result copying, and dirty-navigation protection.
