# ERD Workspace Refresh Design

## Goal

Turn the existing schema diagram into a professional, search-first developer workspace that remains useful as schemas grow. Preserve the current schema API, X6 graph rendering, hidden-table behavior, routes, themes, and pan/zoom interactions.

## Scope

Refresh only the ERD workspace. Add client-side entity search, table/view visibility controls, entity relationship focus, fit/reset controls, polished states, responsive behavior, and focused coverage.

Do not add an explorer sidebar, minimap, inspector panel, saved layouts, schema editing, persistence, backend changes, or new dependencies.

## Workspace and Interaction

The ERD remains canvas-first. A compact workspace header replaces the generic “Schema” title and shows the ERD title plus table, view, and relationship counts.

A single toolbar contains:

- Entity search
- Tables and Views visibility toggles
- Zoom out and zoom in
- Fit view
- Reset layout

Search opens a keyboard-navigable list of matching entities with their qualified schema name and entity type. Selecting a result pans and zooms to that node, highlights its direct relationships, and dims unrelated nodes and edges.

Clicking a node creates the same focused state. Clicking empty canvas or pressing Escape restores the complete diagram. Search and focus do not mutate schema data.

Tables and views are both visible by default. Their toggles remove the corresponding nodes and associated edges. Existing hidden-table behavior remains unchanged, including the special handling for `_user`.

Nodes remain draggable. Panning and wheel zoom remain available. Fit view recalculates around visible entities. Reset layout restores the generated grid before fitting it.

## Visual Direction

The workspace follows the refreshed Tables and SQL Editor visual system: neutral surfaces, compact controls, restrained borders, clear hierarchy, and blue reserved for active focus.

Entity cards use a subtle elevated surface. Their headers contain the qualified entity name and a compact Table or View badge. Column rows retain a dense two-column structure while making keys, nullability, and data types easier to scan.

Relationship lines remain subdued until an entity is focused. Connected nodes and edges become prominent while unrelated graph elements dim without disappearing.

## Components and Data Flow

`ErdPage` owns declarative workspace state:

- Search text and search result visibility
- Table/view visibility
- Selected entity
- Loading, error, and empty states
- Fit and reset commands

`SchemaErdGraph` transforms the fetched schema into graph nodes and edges. The transformation becomes a small exported function that includes entity type and relationship metadata for testing and interaction. Visibility filtering happens before nodes and edges reach X6 so hidden entities cannot leave dangling edges.

`ErdGraph` remains the narrow imperative boundary around X6. It owns graph creation, generated positions, pan/zoom, and node/canvas events. It accepts selection and visibility state through props and reports selection upward. Toolbar controls use minimal command callbacks rather than a generic graph-controller abstraction.

Theme changes rebuild the graph with semantic light and dark colors aligned with the admin theme. Selection styling updates graph attributes without refetching schema data.

## Loading, Error, and Empty States

The header and toolbar shell remain visible while schema data loads, with a centered loading treatment in the canvas.

Fetch failures use a contained callout with a Retry action instead of raw serialized error text.

A schema with no entities shows a purposeful empty state. When filters hide every entity, the empty-state action clears the filters.

## Accessibility

Search supports arrow-key navigation, Enter to focus, and Escape to close results or clear graph focus. Results include entity type and qualified schema name so duplicate names remain distinguishable.

Every toolbar control has an explicit accessible name and visible focus treatment. A live status announces the focused entity and its direct relationship count. Individual SVG column rows are not added to the tab order; search provides the practical keyboard route into the graph.

## Responsive Behavior

Desktop uses one compact toolbar row. On narrow screens, search occupies the first row and filters and navigation controls wrap beneath it. Controls retain touch-sized targets and the canvas consumes the remaining viewport.

Search results overlay the canvas instead of reducing its dimensions. No required control disappears on mobile.

## Testing and Acceptance

Focused tests cover:

- Schema-to-graph transformation
- Table/view filtering
- Removal of dangling edges
- Entity search matching
- Selected-neighborhood calculation
- Toolbar rendering and accessible labels
- Loading, error, and empty states

Browser acceptance covers:

- Search-to-focus behavior
- Node selection and Escape clearing
- Zoom in/out
- Fit and reset layout
- Table/view visibility toggles
- Light and dark themes
- Mobile toolbar wrapping

Final verification runs formatting, TypeScript, ESLint, the complete Vitest suite, the production build, and `git diff --check`.
