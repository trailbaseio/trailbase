# TrailBase Admin UI Refresh Design

## Objective

Refresh the TrailBase admin UI into a professional, cohesive developer tool while preserving its existing SolidJS architecture, behavior, routes, and data flow. The redesign should remain compact and efficient for data-heavy workflows and provide equally considered light and dark themes.

## Direction

The visual direction is a calm, data-dense developer tool inspired by products such as Linear and Supabase. TrailBase blue remains the identifying accent, but neutral surfaces, typography, spacing, and hierarchy do most of the visual work.

The implementation will evolve the current presentation layer rather than replace it. Existing SolidJS, Kobalte, Tailwind, TanStack, and application-specific components will be reused. No dashboard template or new design-system dependency will be introduced.

## Application Shell

Desktop uses a 240px labeled sidebar that can collapse to a 64px icon rail. The sidebar contains:

- TrailBase identity
- Primary navigation grouped by workflow
- Clear active and hover states
- Theme, version, and account controls in the footer

The collapsed preference is stored locally. Mobile keeps a compact top bar and presents navigation in a drawer rather than a horizontally scrolling icon strip.

The main workspace follows a consistent hierarchy:

1. Optional breadcrumb or contextual label
2. Page title and supporting description
3. Primary and secondary actions
4. Page content with an appropriate constrained or full-width layout

Tables, logs, SQL, OpenAPI, and ERD use the available workspace. Forms and settings use readable content widths.

## Visual System

The existing CSS variables become a semantic token system shared by both themes. Components reference roles rather than hard-coded colors:

- background and foreground
- surface/card and elevated popover
- muted surface and muted text
- border and input
- primary and primary foreground
- accent/selection
- success, warning, error, and destructive
- sidebar-specific surfaces and states

Light mode uses neutral slate surfaces with subtle blue undertones. Dark mode uses layered blue-black/slate surfaces rather than a simple inversion. Both themes maintain accessible contrast and identical hierarchy.

Additional rules:

- Use a consistent 4/8px spacing rhythm
- Reserve TrailBase blue for selection, focus, links, and primary actions
- Prefer surface contrast over excessive borders
- Use restrained shadows only for elevated UI
- Use slightly softened corners without turning every control into a pill
- Use tabular numerals for metrics and dense data
- Keep controls compact on desktop and touch-friendly on mobile
- Use short transitions only for navigation, menus, and overlays
- Preserve visible keyboard focus states

## Shared Components

Shared primitives are the source of visual consistency. Buttons, cards, fields, tables, tabs, dialogs, sheets, badges, tooltips, sidebars, callouts, separators, and toasts will be restyled before page-specific cleanup.

A shared page-header pattern will align titles, descriptions, contextual selectors, and actions. A small set of shared loading, empty, and error presentations will replace inconsistent page-level states where practical.

No speculative abstraction will be added. Existing primitives will be extended only when multiple current pages require the same presentation.

## Page Treatment

### Dashboard

Present a concise system overview with useful metrics, recent context, and clear paths to core workflows. Remove internal visual-test content from the user-facing dashboard.

### Tables and Accounts

Use consistent split-view navigation, compact toolbars, readable grids, deliberate bulk actions, and polished empty states. Preserve high information density and current behavior.

### SQL Editor, ERD, Logs, and OpenAPI

Use full-workspace layouts with unified headers and controls. Visual chrome should recede so the primary tool remains dominant.

### WASM and Settings

Use predictable sidebar-detail layouts, grouped forms, concise descriptions, clear save states, and visually isolated destructive actions.

### Authentication

Use a restrained branded login surface with improved hierarchy, spacing, and responsive behavior. Authentication behavior remains unchanged.

## State and Error Handling

Existing queries, mutations, forms, routes, and API calls remain unchanged. This is a presentation and usability refresh.

- Loading states use shared skeleton or progress treatments
- Empty states explain what is missing and offer a relevant next action where one exists
- Recoverable errors appear near affected content
- Toasts remain reserved for transient operation results
- Destructive actions retain confirmation and strong visual separation
- Disabled and pending states remain visually distinct in both themes

## Responsive Behavior

The sidebar collapses into a mobile drawer below the existing mobile breakpoint. Page headers wrap actions predictably, dense tables retain horizontal scrolling where needed, and controls maintain usable touch targets. Mobile layouts should not hide required functionality.

## Validation

- Existing TypeScript, ESLint, and Vitest checks
- Focused tests for sidebar collapse and theme persistence
- Production Vite build
- Browser review at desktop and mobile widths
- Representative page checks in both light and dark themes
- Keyboard navigation review
- Basic contrast review for text, controls, focus rings, and status colors

## Non-Goals

- Backend or API changes
- Route or data-model changes
- Replacing SolidJS, Kobalte, Tailwind, or TanStack
- Adding a dashboard template or new component framework
- Rewriting specialized editor, graph, map, or OpenAPI integrations
- Adding animation beyond restrained interaction transitions
