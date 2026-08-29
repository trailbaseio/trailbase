# Tables Workspace Design

**Date:** 2026-08-28
**Status:** Approved
**Scope:** TrailBase admin UI Tables screen

## Objective

Redesign Tables as a focused database workspace that improves hierarchy, discoverability, information density, and feedback without removing or changing existing backend functionality.

The screen must preserve table and view selection, hidden-table preferences, filtering, sorting, pagination, row insertion and editing, bulk deletion, blob display modes, schema alteration, index management, trigger inspection, Record API configuration, schema download/debug information, and destructive confirmation flows.

## Problems in the Current Screen

- The global sidebar and table picker consume roughly 500px before table content begins.
- A large empty global header wastes vertical space.
- The table picker has no search, schema grouping, counts, or strong table/view distinction.
- Data browsing, structure management, indexes, triggers, and API settings appear in one long page.
- Destructive table deletion is more prominent than common actions.
- Filter syntax is communicated mainly through placeholder text.
- Pagination and bulk actions feel detached from the data grid.
- Permanent pin controls and wrapping technical values reduce usable column space.
- Loading, empty, filtered-empty, and fetch-error states do not communicate clear next actions.

## Information Architecture

The screen uses three regions on large desktops.

### Application Sidebar

Keep the existing global navigation, but make it visually subordinate to the active workspace. Reduce oversized branding treatment while preserving collapse behavior and navigation functionality.

### Table Explorer

Use a fixed 240–260px secondary panel containing:

- Tables heading and total count
- Local search field
- Create table action
- Resources grouped by schema
- Distinct table and view icons
- Show system tables toggle
- Truncated names with full-name tooltips
- Clear selected state

Do not add panel-resizing machinery initially. The explorer can collapse when more grid width is needed.

### Selected-Table Workspace

Remove the empty global header and replace it with a sticky resource header containing:

- Breadcrumb: `Tables / schema / resource`
- Table or View badge
- Compact metadata such as column and row counts
- Contextual primary action
- Overflow menu for infrequent and destructive actions

Below the header, organize functionality into URL-addressable tabs:

- **Data:** rows, filtering, sorting, pagination, insertion, editing, and deletion
- **Structure:** columns, constraints, indexes, and triggers
- **API:** Record API status and configuration

Table deletion moves into the resource overflow menu and retains its existing confirmation flow.

## Data Tab

### Toolbar

Use one horizontal toolbar with:

- Filter expression input
- Apply and Clear controls
- Syntax-help popover with copyable examples
- Refresh action and loading status
- Insert row as the primary action
- View menu for blob encoding and column visibility

Pressing `/` focuses the filter. Enter applies it when focus is in the filter field.

### Data Grid

- Use sticky column headers and compact, consistent row heights.
- Scroll horizontally rather than wrapping technical values.
- Pin the primary key by default.
- Show sorting clearly only on active columns.
- Move pinning and other column actions into header menus.
- Give NULL, boolean, blob, foreign-key, and JSON values restrained, distinct formatting.
- Truncate long UUIDs and values visually while preserving copy and full-value actions.
- Keep foreign keys navigable.
- Open the existing edit form in a side sheet when a row is activated, preserving table context.

Selection checkboxes remain available. The destructive bulk action appears only after rows are selected, in a contextual bar such as:

> 3 rows selected · Delete · Clear selection

### Pagination

Place pagination below the grid:

- Total row count on the left
- Page size and first/previous/next/last controls on the right
- Clear disabled states

### Data States

- Loading: fixed-height skeleton rows to prevent layout shifts
- Empty table: “No rows yet” with Insert first row
- Empty filtered result: “No rows match this filter” with Clear filter
- Fetch error: inline explanation and Retry while preserving the surrounding workspace

## Structure Tab

Start with a compact summary showing:

- Resource type
- Schema-qualified name
- Primary key
- Column, index, and trigger counts

Alter table is the primary Structure action. Views and other read-only resources show a clear read-only state instead.

### Columns

Present columns as the main structured list with:

- Name
- Data type
- Nullable state
- Default value
- Primary- and foreign-key indicators
- Unique constraints

Avoid unnecessary wrapping. Foreign-key targets link to the referenced table. Preserve the existing safe-sheet alter flow.

### Indexes and Triggers

Use separate bordered sections.

- Indexes retain add, select, and delete functionality.
- Triggers show name and SQL statement.
- Where trigger modification remains unsupported, provide an Open SQL Editor action alongside the explanation.

Move schema download and debug information into the resource overflow menu.

## API Tab

Give Record API configuration a dedicated tab containing:

- Clear enabled or disabled status
- Requirement warnings for unsupported tables or views
- Existing create/read/update/delete configuration
- Authentication and conflict-resolution settings
- Validation errors near affected fields
- Explicit save progress and success feedback

When enabled, display the resource path and common endpoint patterns with copy controls, using existing configuration and route information. Do not introduce a separate API system.

## Responsive Behavior

### Large Desktop

- Global sidebar can be expanded or collapsed to its icon rail.
- Table explorer uses a fixed 240–260px width.
- Workspace receives all remaining width.
- Data grid scrolls inside its own region.

### Medium Desktop and Tablet

- Global sidebar defaults to the icon rail.
- Table explorer remains available and can collapse.
- Resource header contains a persistent explorer toggle.

### Mobile

- Show only the selected-resource workspace by default.
- Open global navigation and table explorer as separate drawers.
- Collapse resource actions into an overflow menu.
- Keep tabs horizontally scrollable.
- Preserve the database grid with horizontal scrolling rather than converting rows into cards.

## State and URLs

- Preserve existing `/table/:table` routes.
- Store the active workspace tab in `?tab=data|structure|api`.
- Preserve existing URL/query behavior for filters, sorting, and pagination.
- Continue persisting selected-table and hidden-resource preferences locally.
- Keep shell-sidebar and table-explorer collapse preferences independent.

## Implementation Boundaries

- No backend or API changes
- No new dependencies
- Reuse SolidJS, TanStack Table, Kobalte, existing sidebar, tabs, sheets, dialogs, and forms
- Preserve every existing Tables action and safety flow
- Refactor only where needed to separate Data, Structure, and API presentation
- Do not create a generic workspace framework until another screen proves it reusable

## Validation

- Unit tests for tab URL state, explorer filtering, and contextual bulk actions
- Existing functional tests retained
- TypeScript, ESLint, Vitest, formatting, and production build
- Visual checks at expanded desktop, collapsed desktop, tablet, and mobile widths
- Manual verification of every existing Tables action before approval
