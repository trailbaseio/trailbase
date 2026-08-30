# Logs Workspace Refresh Design

## Goal

Turn Logs into a compact request-investigation workspace where administrators can scan traffic, filter requests, and inspect one request without losing table context.

The refresh preserves the existing SolidJS stack, routes, APIs, filter expression syntax, URL parameters, server-side sorting, cursor pagination, Chart.js graph, MapLibre GeoIP map, and manual refresh behavior.

## Current problems

- The 300px activity graph dominates the viewport and pushes the request table below the fold.
- Ten equally weighted columns make important request data hard to scan and force excessive horizontal scrolling.
- Status, method, URL, and latency lack hierarchy.
- Full user agents and secondary metadata occupy primary table space.
- Rows do not provide a focused inspection workflow.
- Chart and map controls are icon-only and difficult to discover.
- List failures reset filter and pagination state.
- List and statistics failures are coupled even though request browsing can work without insights.
- Empty logs and zero filtered matches are indistinguishable.

## Approved direction: request inspector

Logs will be table-first, with compact activity insights above it and a request detail sheet for complete metadata.

### Header

The header contains:

- Logs title.
- Filtered request count when available.
- Clearly labeled manual Refresh action.

Refresh updates the list and statistics together. It visibly reports progress while preserving current data and URL state.

### Activity insights

The existing request-rate graph and GeoIP map remain available in a compact, explicitly labeled Activity section.

- Approximately 180px tall on desktop.
- Collapsible with a discoverable text label.
- Open by default on desktop and collapsed by default on mobile.
- Graph and map share the row at wide widths when GeoIP data exists.
- Mobile layouts stack content or defer the map until Activity is opened.
- Missing GeoIP data uses inline setup guidance rather than an unexpected modal.
- Statistics failures remain local to Activity and never block the request table.

Chart.js and MapLibre use element references instead of global DOM IDs. Instances are destroyed before replacement and on unmount. Theme colors come from semantic CSS variables. Non-text visualizations receive accessible labels or summaries.

### Request table

The primary table is dense and prioritizes:

1. Time
2. Status
3. Method
4. Request URL
5. Latency
6. Client
7. User

Status and method use compact semantic badges. Latency uses readable adaptive formatting rather than six decimal places. Request URL is the dominant field. Client geography may appear as secondary text.

Referer, full user agent, exact timestamps, internal ID, and full geographic data move to request inspection. The table keeps semantic headers, server-side sorting, pagination, keyboard-operable rows, selectable values, and contained horizontal overflow.

### Request detail sheet

Selecting a row opens a right-side sheet on desktop and a full-width sheet on mobile. The table remains mounted behind it.

The sheet exposes every existing `LogJson` field in grouped sections:

- Request
- Timing
- Client and location
- Identity
- Metadata

Useful values have copy actions. Success feedback names the copied field without echoing potentially sensitive values. Focus moves into the sheet and returns to the originating row when it closes.

There is no editing, deletion, export, replay, or separate request-by-ID API.

## Filtering and URL state

The existing filter expression remains authoritative and URL-backed through:

- `filter`
- `pageIndex`
- `pageSize`

The `/` keyboard shortcut continues to focus the filter. A small Filter syntax disclosure provides examples for status, method, latency, URL, client IP, and user ID without introducing a second filter model.

Applying or clearing a filter resets pagination to page one and updates both the list and statistics. Server-side sorting also resets pagination. Sorting remains local, matching current behavior. Cursor bookkeeping stays internal.

Errors must not clear filters, pagination, or sorting.

## State model

List and insights queries render independently:

- Initial list loading: stable skeleton rows with the toolbar visible.
- List refresh: retain current rows and show progress.
- List error: retain the workspace and offer Retry.
- Initial empty state: explain that no requests have been recorded.
- Filtered empty state: explain that no requests match and offer filter clearing.
- Statistics loading/error/empty: contained inside Activity.
- Request count: reflects the active filter.

No raw backend error details should be disclosed in user-facing copy.

## Responsive behavior

### Desktop (`1440×900`)

- Fixed admin sidebar remains visible.
- Compact Activity region leaves the table visible above the fold.
- Detail sheet opens from the right.
- Table overflow remains inside the workspace.

### Mobile (`390×844`)

- Activity starts collapsed.
- Header and filter controls wrap without body-level overflow.
- The semantic table scrolls within its own container.
- Detail inspection uses a full-width sheet.
- Touch targets and focus states remain usable.

## Architecture

- `LogsPage.tsx`: URL state, list/statistics queries, table construction, refresh, and workspace states.
- `LogsInsights.tsx`: graph, map, GeoIP guidance, and visualization lifecycle.
- `LogDetailsSheet.tsx`: selected-request inspection and copy interactions.
- Small pure formatting/status helpers may live with the Logs workspace and receive direct unit coverage.

Reuse existing `Header`, `Table`, `FilterBar`, `Sheet`, `Badge`, `Button`, `Callout`, Tooltip, TanStack Query/Table, and Kobalte primitives. Add no dependencies.

## Validation

Automated checks cover:

- Timestamp, method, status, client, and latency presentation.
- URL-backed filtering and pagination resets.
- Query keys and cursor behavior.
- Independent list and insights loading/error states.
- Refresh without URL-state loss.
- Empty versus no-match messaging.
- Keyboard row activation and sheet focus restoration.
- Complete detail fields and safe copy feedback.
- Chart and map instance cleanup.
- Contained responsive overflow.

Browser acceptance covers desktop light/dark at `1440×900`, mobile at `390×844`, Activity collapse, graph/map behavior, filtering, sorting, pagination, request inspection, focus, loading/error states, and console/network output.

## Out of scope

- Auto-refresh or live tailing.
- Backend changes or new endpoints.
- Saved filters.
- Export.
- Request replay.
- Log deletion.
- New filtering syntax.
- New dependencies.
- Persisted Activity or selected-row state.
