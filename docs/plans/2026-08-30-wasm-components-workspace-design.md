# WASM Components Workspace Design

**Date:** 2026-08-30
**Status:** Approved

## Goal

Turn the WASM Components page into a professional, data-dense component registry while preserving component discovery, install/remove operations, dashboard routes, and the existing iframe security boundary.

## Constraints

- Preserve SolidJS, TanStack Query, Kobalte, Tailwind, existing routes, APIs, and generated bindings.
- Preserve `/_/admin/wasm` and `/_/admin/wasm/:name`.
- Preserve install/remove behavior and the requirement to restart the server before loaded state changes.
- Preserve iframe sandboxing, CSP, same-origin path validation, token messaging, and the development-only sandbox toggle.
- Add no dependencies or backend changes.
- Do not add search, bulk actions, registry editing, or automatic restart controls.

## Workspace Structure

The list page becomes a compact registry rather than a card gallery.

The header shows:

- **WASM Components**
- A summary such as `4 components · 2 running`
- A refresh action

A compact informational callout retains the CLI guidance. When any component's loaded and installed states differ, the callout becomes a prominent **Restart required** warning.

The registry has five columns:

1. **Component** — icon, display name, internal name, and one-line description
2. **State** — current runtime/filesystem relationship
3. **Runtime / Version**
4. **Source** — repository ID or local WASM path
5. **Actions** — Open dashboard, Install, or Remove

Pending-restart components sort first, followed by running and available components. Each group sorts by display name. No search is included because the registry is currently small and fixed.

The entire row is not clickable. Components with dashboards expose an explicit **Open dashboard** action so row management controls remain unambiguous.

## Component State Model

A small pure helper derives presentation state from the existing booleans:

| Loaded | Installed | State |
| --- | --- | --- |
| true | true | Running |
| false | false | Available |
| false | true | Install pending restart |
| true | false | Removal pending restart |

The state uses text and iconography in addition to color. No backend fields change.

## Query and Routing Ownership

`WasmPage` becomes the single owner of the `['wasm-components']` query. It handles:

- Loading
- Query failure and retry
- Empty registry
- Component list
- Component detail routing
- Missing component routes

It passes component data and refresh behavior downward, removing the current duplicate list query.

The detail route remains a full-width embedded dashboard. Its header retains Back, component name/version, and the development-only sandbox toggle.

## Install and Remove Safety

Install and remove operations are awaited. While a mutation is running:

- The affected action is disabled
- Progress text is shown
- Duplicate submissions are blocked

On success, the confirmation closes and the registry refreshes. The resulting loaded/installed skew is surfaced as a restart-required state.

On failure, the confirmation stays open and shows safe, actionable feedback without exposing raw server details.

Remove confirmation names the exact component and explains that the currently loaded component continues running until restart.

## Complete States

The workspace retains its header and guidance through all states.

- **Loading:** compact registry skeleton or centered progress within the table region
- **Error:** safe failure callout with Retry
- **Empty:** explanation plus the existing CLI installation command
- **No dashboard:** clear component-specific state with return action
- **Unknown route:** component-not-found state with return action
- **Dashboard fetch failure:** safe embedded-workspace error with retry
- **Restart required:** prominent warning identifying that runtime and filesystem state differ

## Responsive Behavior

Desktop shows all five columns.

On narrow screens, the registry remains data-complete inside a bounded horizontal scroll region instead of hiding state or source information. Component and State remain the leading columns. Header controls and guidance stack cleanly.

The embedded dashboard fills the available workspace on desktop and mobile.

## Accessibility

- Use semantic table headers and rows.
- Give every icon-only control an accessible name and matching title.
- Keep source paths selectable.
- Ensure dashboard actions are keyboard reachable.
- Use descriptive confirmation titles and explicit action labels.
- Return focus to the trigger after a dialog closes.
- Announce mutation progress and failure.
- Keep component-specific iframe titles.
- Never communicate component state through color alone.

## Security

The refresh does not broaden component dashboard permissions.

Preserve:

- Sandboxed `srcdoc` execution
- Existing CSP
- Same-origin path validation
- Token setup via `postMessage`
- Development-only unsafe iframe toggle

## Validation

### Pure helper tests

- All four loaded/installed state combinations
- State labels and priority ordering
- Stable display-name ordering
- Runtime/version/source presentation

### UI tests

- Loading
- Error and Retry
- Empty registry
- Unknown component route
- Missing dashboard
- Restart-required guidance
- Install/remove confirmation
- Mutation progress
- Success refresh
- Failure retention and safe errors
- Accessible names and titles

### Browser acceptance

Validate desktop light/dark and `390×844`:

- Registry density and horizontal access
- State presentation
- Dashboard navigation and Back
- Development sandbox toggle
- Confirmation cancellation
- Keyboard focus
- Console and network errors

## Explicitly Out of Scope

- Search and filtering
- Bulk install/remove
- Registry editing
- Automatic server restart
- Backend or API changes
- New dependencies
- New iframe permissions
- A split-pane dashboard
