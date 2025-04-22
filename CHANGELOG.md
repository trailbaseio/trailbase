## v0.9.4

* Overhaul insert/update row/record form:
  * Integer primary keys are nullable.
  * Explicit nullability for numbers.
  * Ignore defaults for update path.
  * Don't pre-fill defaults.
* Install SIGHUP handler for config reload.
* Update to Rust edition 2024.
* Update dependencies.

## v0.9.3

* Custom JSON stdout request logger to have a stable format as opposed to
  depending on the span/event structure, which is an implementation detail.
* Show response timestamps in dashboard with millisecond resolution.
* Log response timestamp explicitly.
* Improve logs writer performance: no transaction needed, improved statement
  caching.
* Improve incremental release build times by ~70% switching from "fat" to "thin" LTO.
* Update dependencies.

## v0.9.2

* Overhaul SQLite execution model to allow for parallel reads. This should help
  reduce latency long-tail with slow queries.
  * And add more benchmarks.
* Log request/response logs to stdout in JSON format.
* Always re-create traildepot/.gitignore. Previously gated on creating the root
  path, which was never the case for docker users.
* Update dependencies.

## v0.9.1

* Consistently expanded JSON schemas for specific APIs everywhere (UI & CLI).
* Improved foreign table id lookup during schema evaluation.
* Stricter SQL validation in admin UI.
* Break up sqlite and core creates into two additional crates: schema & assets.
* Update dependencies.

## v0.9.0

* Performance:
  * Read and write latency improved both by ~30% 🔥.
  * Memory footprint dropped by ~20% in our insert benchmarks.
  * Build narrower INSERT queries.
  * Use more cached statements.
* Overhaul object-store/S3 file life-cycle/cleanup.
  * Use triggers + persistent deletion log.
  * Retry cleanups on transient object store isues.
  * Fix issue with zombie files on UPSERTs.
* Decouple record APIs form underlying TABLE/VIEW schemas.
* Fix leaky abstractions by pushing tracing initialization into server
  initialization and more strictly separate from logging.
* Update dependencies.

## v0.8.4

* Add a `?loginMessage=` query parameter to admin login page.
* Move query construction for more complex queries to askama templates and add more tests.
* Move subscription-access query construction from hook-time to RecordApi build-time.
* Use askama for auth UI.

## v0.8.3

* Support more SQL constructs:
  * Conflict clause in table and column unique constraints.
  * FK triggers in column constraints.
  * CHECK table constraints.
* Fix: pagination cursors in list queries for arbitrary PKs.
* Sanitize expand and order column names in list queries.
* Update dependencies.

## v0.8.2

* Quote table/index/column names during "CREATE TABLE/INDEX" parsing and construction.
* Improve auth UI: more consistent shadcn styling and explicit tab orders.
* UUID decode sqlite extension and more consistent extension names.
* Update deps.

## v0.8.1

* Derive job id in native code for JS/TS jobs.
* Fix conflict resolution selector in admin UI's API settings.
* Fix primary key card collapsing in create table form.

## v0.8.0

* Add support for periodic cron jobs:
  * Add dashboard to admin UI to inspect, configure and trigger cron jobs.
  * Users can register their own cron jobs from the JS runtime.
  * Replace internal periodic tasks with cron jobs to increase configurability,
    discoverabilty, and avoid drift.
  * BREAKING: removed `backup_interval_sec` from proto config. When explicitly specified,
    users will need to remove it from their `<traildepot>/config.textproto` and set an
    appropriate cron schedule instead.

## v0.7.3

* Cleanup logs DB schema and log ids of authenticated users.
* Allow setting the name and INTEGER type for PKs in create table form.
* Fix reactivity for FK settings in create/alter table forms.
* Add confirmation dialog for user deletions.
* Limit mutations in `--demo` mode.
  * Dedicated admin delete user endpoint.
* Unified parameter building for listing records, users and logs.
* Cleanup backend auth code and query API.
* Update dependencies including rusqlite.

## v0.7.2

* Fix and test OpenId Connect (OIDC) integration.
* Audit and remove unwraps.
* Fix auth img-src CSP for external avatars and dev instances.

## v0.7.1

* Add generic OIDC provider. Can currently only be configured in config. Admin UI integration pending.
* Add --demo mode to protect PII in demo setup.
* Improve secrets redaction/merging.

## v0.7.0

* Schema-aware auto-completion in SQL editor.
* Allow UUID text-encoded 16byte blobs as record ids and in filters during record listing.
* Redact secrets in admin APIs/UI to reduce surface for potential leaks.
* Polish auth/admin UI with image assets for external auth providers like discord, gitlab, ... .
* Permissive `img-src` CSP in auth UI to allow displaying avatars from external auth providers.

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
