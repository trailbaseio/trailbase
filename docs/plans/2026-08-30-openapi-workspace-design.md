# OpenAPI Workspace Refresh Design

## Objective

Turn the existing unframed RapiDoc document into a professional API explorer that prioritizes endpoint discovery while preserving TrailBase's current OpenAPI route, generated specification, authentication behavior, request execution, and dependency set.

## Direction

Use RapiDoc's native `focused` mode rather than building a second endpoint browser or replacing the renderer. Desktop presents RapiDoc's own searchable endpoint navigation beside one selected operation. Mobile presents that same navigation as an overlay so documentation retains the full viewport width.

This remains a presentation and usability refresh. No backend, route, generated-specification, or dependency changes are included.

## Workspace Structure

The `/openapi` route becomes a full-height workspace with a compact TrailBase header containing:

- **OpenAPI Explorer** title and concise description
- API version and operation count
- Current server URL
- **Refresh specification** action
- Current admin-session authentication status
- **Advanced authentication** disclosure for optional impersonation tokens

RapiDoc fills the remaining workspace. Its generated documentation remains the dominant surface rather than being wrapped in decorative cards.

## Endpoint Exploration

Configure RapiDoc with:

- `focused` rendering on desktop
- native side navigation and search
- compact navigation spacing
- path-based labels
- colored HTTP-method markers
- table schemas
- inline Try support
- hidden RapiDoc header, authentication UI, and server selector

Tag groups start collapsed. A local presentation clone adds RapiDoc's supported `x-tag-expanded: false` hint without mutating the fetched source object or filtering any operation.

Selecting an endpoint displays only that operation's description, parameters, request body, schemas, examples, responses, cURL output, and request console.

## Authentication

The current TrailBase admin session is the default request identity. The header communicates that state without exposing credentials.

Optional impersonation moves into an **Advanced authentication** disclosure. The value remains a password input and accepts the same base64-encoded token payload copied from Accounts. A valid override must decode to an object containing string `auth_token`, `refresh_token`, and `csrf_token` values.

Invalid input produces a concise field-level message and cannot create malformed request headers. Clearing the override immediately restores current-session authentication. Every Try request receives exactly one Authorization, Refresh-Token, and CSRF-Token header. Copy or validation feedback never renders credential values.

## Data Flow

A single TanStack query owns `GET /openapi.json`. Its stable key allows explicit refresh while preserving already loaded data.

Small pure helpers derive:

- API title and version
- operation count
- collapsed-tag presentation clone
- validated impersonation tokens

A successful response is passed to RapiDoc's existing `loadSpec()` method. No custom OpenAPI parser, operation filtering, or schema renderer is introduced.

RapiDoc event listeners are registered once and removed during cleanup. Theme changes update supported RapiDoc theme and palette attributes without reloading the specification or resetting the selected endpoint.

Development continues to use `http://localhost:4000` as the request server override. Production continues to use the specification's server behavior.

## Loading, Refresh, and Errors

Initial loading shows a compact workspace skeleton.

An initial fetch failure shows a generic **Unable to load the API specification** callout with Retry. Raw backend errors are not exposed.

A failed refresh keeps the existing explorer usable and shows an inline warning near the header. Refresh has a visible pending state and does not clear the selected operation or authentication input.

## Responsive Behavior

Desktop keeps RapiDoc's native focused navigation visible as a secondary workspace sidebar.

At mobile width, the same native navigation becomes an overlay opened by a **Browse endpoints** button. The overlay retains RapiDoc's search and grouped endpoint list; no duplicate mobile navigation state is created. Closing it returns the full width to operation documentation.

Long paths, schema tables, examples, and response bodies remain internally scrollable and must not widen the application shell.

## Accessibility

- Header controls have explicit names and visible focus states.
- Authentication status and errors use text rather than color alone.
- The impersonation field has a persistent label, description, and associated validation message.
- Loading and refresh status are announced without moving focus.
- Mobile endpoint navigation has an explicit open/closed state.
- RapiDoc's native keyboard-accessible search, tag, operation, schema, and Try controls remain available.

## Testing

Automated coverage will verify:

- metadata and operation counting
- non-mutating collapsed-tag presentation cloning
- valid, invalid, and cleared impersonation tokens
- exact request-header injection
- loading, initial error, Retry, retained-data refresh error, and success states
- required focused/search/navigation/schema/Try configuration
- event registration and cleanup
- theme updates without specification reload
- mobile endpoint-browser state

Browser acceptance will cover desktop `1440×900` in light and dark themes and mobile `390×844`, including endpoint search, group expansion, endpoint selection, schemas, current-session Try, impersonated Try, refresh, errors, overflow containment, keyboard focus, console output, and network activity.

## Non-Goals

- Backend or OpenAPI generation changes
- Route changes
- A custom OpenAPI renderer
- A duplicate custom endpoint index
- Saved requests, request history, environments, or collections
- Persisted impersonation credentials
- New dependencies
- Replacing RapiDoc
