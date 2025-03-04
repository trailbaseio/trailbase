## v0.6.8

* Fix client-side merging of fetch arguments including credentials.
* Improved auth UI styling.

## v0.6.7

* Improve token life cycle for JS/TS clients including admin dash.

## v0.6.6

* Add a dialog to avoid accidentally discarding unsaved changes in the SQL editor.
* Polish UI: animate buttons, consistent refresh, avoid logs timestamp overflow.
* Update Rust and JS deps.

## v0.6.5

* Fix routing issues with auth UI.
* Redirect /login to /profile on already logged in.
* Redirect /register to /login?alert= on success.
* Persist execution result in Admin SQL editor.
* Address linter issues.
* Update dependencies.

## v0.6.4

* Add undo history to query editor and improve error handling.
* Cosmetic improvements of Admin UI like more consistency, more accessible buttons, ...
* Indicate foreign keys in table headers.
* Turn table into a route parameter and simplify state management.
* Fix hidden table UI inconsistency.
* Fix input validation issues in form UI.
* Limit cell height in Table UI.

## v0.6.3

* Allow downloading JSON schemas from the UI for all modes: Insert, Update, Select.
* Add some more UI polish: tooltips, optics, and tweaks.
* Improve UI type-safety

## v0.6.2

* Update to address broken vite-plugin-solid: https://github.com/solidjs/vite-plugin-solid/pull/195.

## v0.6.1

* Fix config handling in the UI.
* Improve form handling in the UI.
* Few minor UI fixes & cleanups.
* Update dependencies.

## v0.6.0

* Support foreign record expansion. If a record API is configured allow
  expansion of specific foreign key columns, clients can request to expand the
  parent record into the JSON response of RecordApi `read` and `list`. This is
  also reflected in the JSON schema and warrants a major version update.
  Updates to all the client packages have already been pushed out.
* Support for bulk record creation. This is particularly useful when
  transactional consistency is advisable, e.g. creating a large set of M:N
  dependencies.
* Record subscriptions now have to be explicitly enabled in the
  admin-UI/configuration
* Simplify PNPM workspace setup, i.e. get rid of nesting.
* Fixed rustc_tools_util upstream, thus drop vendored version.
* Reduce logs noise.
* Update dependencies.

## v0.5.5

* Fix build metadata release channel and include compiler version.
* Admin UI: Avoid triggering table's onClick action on text selection.
* Update deps.

## v0.5.4

* Add a `?count=true` query parameter to RecordApi.list to fetch the total
  number of entries.
* Return error on invalid list queries rather than skipping over them.
* Address Admin UI issues:
 * Stale config after altering schema or dropping table.
 * Out-of-sync filter bar value.
 * Reset filter when switching tables.
* Hide "sqlite_" internal tables in Admin UI.

## v0.5.3

* Built-in TLS support.
* Add "server info" to the admin dashboard, e.g. including build commit hash.
* Update deps.

## v0.5.2

* Add file-system APIs to JS/TS runtime to facility accessing resources, e.g.
  templates for SSR (see example/colab-clicker-ssr).
* Add a timeout to graceful shutdown to deal with long-lived streaming connections.
* Allow short-cutting above timeout by pressing a Ctrl+C second time.

## v0.5.1

* Update SQLite from 3.46.1 to 3.48.0.

## v0.5.0

* Breaking change: RecordApi.list now nests records in a parent structure to
  include cursor now and be extensible for the future.
* Update all the client libraries to expect a ListResponse.

## v0.4.1

Minor update:

* Fix issue with delete table potentially invalidating config due to stale RecordAPI entries.
* Update dependencies.

## v0.4.0

Added an early version of Record change subscriptions, a.k.a. realtime, APIs.
Users can now subscribe to an entire API/table or specific record to listen for
changes: insertions, updates, deletions (see client tests, docs are TBD).

## v0.3.4

* Update Axum major version to v0.8.
* Major overhaul of project structure to allow for releasing crates.

## v0.3.3

* Pre-built Windows binary.

## v0.3.2

* Move record API access query construction to RecordApi construction time.
* Cache auth queries
* Some tweaks and hooks API for trailbase_sqlite::Connection.
* Remove sqlite-loadable and replace with rusqlite functions.
* Reduce allocations.

## v0.3.1

* Fix client-ip logging.
* Wire request-type into logs

## v0.3.0

A foundational overhaul of SQLite's integration and orchestration. This will
unlock more features in the future and already improves performance.
Write performance roughly doubled and read latencies are are down by about two
thirds to sub-milliseconds 🏃:

* Replaced the libsql rust bindings with rusqlite and the libsql fork of SQLite
  with vanilla SQLite.
 * The bindings specifically are sub-par as witnessed by libsql-server itself
   using a forked rusqlite.
 * Besides some missing APIs like `update_hooks`, which we require for realtime
   APIs in the future, the implemented execution model is not ideal for
   high-concurrency.
 * The libsql fork is also slowly getting more and more outdated missing out on
   recent SQLite development.
 * The idea of a more inclusive SQLite is great but the manifesto hasn't yet
   manifested itself. It seems the owners are currently focused on
   libsql-server and another fork called limbo. Time will tell, we can always
   revisit.

Other breaking changes:

* Removed Query APIs in favor of JS/TS APIs, which were added in v0.2. The JS
  runtime is a lot more versatile and provides general I/O. Moreover, query APIs
  weren't very integrated yet, for one they were missing an Admin UI. We would
  rather spent the effort on realtime APIs instead.
  If you have an existing configuration, you need to strip the `query_apis`
  top-level field to satisfy the textproto parser. We could have left the
  field as deprecated but since there aren't any users yet, might as well...

Other changes:

* Replaced libsql's vector search with sqlite-vec.
* Reduced logging overhead.

## v0.2.6

* Type JSON more strictly.
* Fix input validation for nullable columns in the insert/edit row Admin UI form.

## v0.2.5

* Addresses issues reported by reddit user *qwacko* 🙏
  * Fix serialization of foreign key column options.
  * Fix deserialization of TableIndex.
  * Admin UI: Show all tables, including hidden ones, in create-table-form's
    drop down for column foreign-keys.

## v0.2.4

* Allow configuring S3 compatible storage backend for file uploads.

## v0.2.3

* Interleaving of multiple HTTP requests into busy v8 isolates/workers.
* JS runtime:
  *  add `addPeriodicCallback` function to register periodic tasks that
     executes on a single worker/isolate.
  *  Constrain method TS argument type (`MethodType`).

## v0.2.2

* Enable "web" APIs in JS runtime.
* Add "Facebook" and "Microsoft" OAuth providers.

## v0.2.1

* Allow setting the number V8 isolates (i.e. JS runtime threads) via
  `--js-runtime-threads`.

## v0.2.0

* Add JS/ES6/TS scripting support based on speedy V8-engine and rustyscript runtime.
  * Enables the registration of custom HTML end-points
  * Provides database access.
  * In the future we'd like to add more life-cycles (e.g. scheduled
    operations).
  * In our [micro-benchmarks](https://trailbase.io/reference/benchmarks/) V8
    was about 45x faster than goja.
* Added official C#/.NET client. Can be used with MAUI for cross-platform
  mobile and desktop development.

## v0.1.1

* Changed license to OSI-approved weak-copyleft OSL-3.0.
* Add GitHub action workflow to automatically build and publish binary releases
  for Linux adm64 as well as MacOS intel and apple arm silicone.
* Add support for geoip database to map client-ips to countries and draw a world map.

## v0.1.0

* Initial release.
