# TrailBase Auth UI WASM Component

The default Auth UI that is (used to be) built into TrailBase as a separate
WASM component.

Building your own Auth UIs was always an option, both client side and
server-side. Unbundling the Auth UI,

* Makes it easier to customize and build your own.
* Demonstrates the composability of WASM components along others as sort of an
  early-day plugin system.
* You don't pay the cost if you don't use it.

## Implementation Details

This crate mostly uses plain HTML forms and server-side rendering (the user's
profile page uses JavaScript) to allow even no-script users to sign in.
Astro is used in SSG-mode to generate the templates, which are then rendered by
the Rust WASM component using the askama crate.
