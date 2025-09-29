## v0.18.2

* Add support for record-filters for realtime subscriptions to Dart and .NET clients.
* Fix parsing of implicit `$and` filter on expressions touching the same column, e.g. `?filter[col0][$gt]=0&filter[col0][$lt]=5`. Thanks @kokroo 🙏.
  * Also further streamline the parsing and allow single element sequences in accordance with QS.
* Restore ability to parse record inputs for `STRICT` tables with `ANY` columns.
* Fix blog example after `FileUpload` changes in previous release (which should have arguably been a major release :hide:)
* Update Rust dependencies.

## v0.18.1

- Allow realtime subscriptions to define record-based filters 🎉. This could be used, e.g. to subscribe to only to changes within a GIS bounding-box. The query API is consistent with listing records. The TypeScript client has been updated to support it.
- Create unique filenames for uploads: `{stem}_{rand#10}.{ext}` and allow accessing their contents using said filename.
  - We may want to deprecate indexed-based access in the future - there's clear advantages for stable unique ids, e.g. for content caching strategies when using a proxy or CDN.
- Address issue with `@tanstack/form`s missing `crypto.randomUUID` #157.
- Update Rust and JavaScript dependencies.

## v0.18.0

- If everything goes to [plan](https://trailbase.io/blog/switching_to_a_wasm_runtime), v0.18.x will be the last releases containing the v8 JavaScript engine. It's time to move to WASM. If you have any issues or encounter any limitations, please reach out 🙏.
- **Remove built-in auth UI** in favor of a matching WASM component. To get the auth UI back, simply run: `trail components add trailbase/auth_ui`. Why did we remove it?
  - Moving the UI into a component in `crates/auth-ui` offers a starting point for folks to **customize or build their own**. We plan to further streamline the process of customization, both in structure and documentation.
  - The component serves as a proof-of-concept for the new WASM compositional model, our commitment and also makes us eat our own dogfood.
  - We hope that the current, extremely basic component management may inform a possible future of a more open plugin ecosystem. We would love to hear your feedback 🙏.
- Update Rust dependencies.

## v0.17.4

- Fix/change JSON file-upload to expect contents as (url-safe) base64 rather than verbose JSON arrays. Also add integration tests - thanks @ibilux 🙏.
- Add very basic WASM component management functionality to the CLI, e.g.: `trail components add trailbase/auth_ui`.
  - Note that this is far from being a third-party plugin system. For now this is barely more than a short-hand to install the auth-ui component in a versioned fashion.
- Add a new `traildepot/metadata.textproto` to persist ephemeral metadata in human-readable form - currently only the previously run version of `trail` to detect version transitions.
- Add an argument to the WASI `init-endpoint` for future-proofing and to pass metadata, e.g. to let a component validate itself against the host version.
- Update Rust dependencies and toolchain to v1.90.

## v0.17.3

- Reload WASM components on SIGHUP in dev mode.
- Fix JSON schema construction for schemas with nested references.
- Small styling fixes for admin UI.
- Update `wstd`, publish updated `trailbase-wasm` crate and update JavaScript dependencies.

## v0.17.2

- PoC: release built-in Auth UI as separate WASM component. In the future we may want to unbundle the UI and allow installing it as a plugin.
- Allow disabling WASM runtime via feature flag in the core crate.
- Improved Rust guest runtime: query param de-serialization and response.
- Update Rust dependencies.

## v0.17.1

- Add an SMTP encryption setting (plain, STARTTLS, TLS).
- Migrate all UIs to Tailwind v4.
- Fix job name encoding for WASM jobs.
- Make examples/wasm-guest-(js|ts|rust) standalone to just be copyable.
- Update Rust & JavaScript dependencies.

## v0.17.0

- Add new WASM runtime based on [wasmtime](https://github.com/bytecodealliance/wasmtime). More information in our dedicated update [article](https://trailbase.io/blog/switching_to_a_wasm_runtime).
  - This is a transitional release containing both the V8 and WASM runtime. The plan is to eventually remove V8.
  - We expect WASM to unlock a lot of opportunities going forward, from increased performance (though JS is slower), strict state isolation, flexible guest language choice, more extensibility... if we peaked your interest, check out the article.
- Update JavaScript and Rust dependencies.

## v0.16.9

- Add support for non-transactional bulk edits to experimental "transaction record API" and expose it in the JS/TS client. Thanks so much @ibilux 🙏.
- Update Rust dependencies.

## v0.16.8

- Allow signed-out change-email verification. This allows validation from a different browser or device, e.g. pivot to phone.
- Fix change-password auth flow.
- Update JS dependencies.

## v0.16.7

- Fix and improve email-template forms in admin UI
  - Fix mapping of form to config fields
  - Allow individual setting of subject or body as well es un-setting either.
  - Check that body contains `{{CODE}}` or `{{VERIFICATION _URL}}`.
- Update Rust dependencies.

## v0.16.6

- Fix slow startup of streaming connections. Previously, clients would consider connections established only after first observed change or heartbeat. Thanks @daniel-vainsencher and meghprkh!
- Fix update-params in experimental record transaction API. Thanks @ibilux!
- Minor: improve tb-sqlite connection setup and support arc locks (backport from ongoing WASM work).
- Simplify JS runtime's handling of transactions.
- Update Rust dependencies.

## v0.16.5

- Add an experimental `/api/transaction/v1/execute` endpoint for executing multiple record mutations (create, update & delete) across multiple APIs in a single transaction.
  - Disabled by default and needs to be enabled in config file (no UI option yet).
  - There's no client-integrations yet. We're looking into how to best model type-safety for multi-API operations.
- Update to JS/TS runtime to deno 2.3.3 level.
- Update dependencies.

## v0.16.4

- Switch to dynamically linked binaries on Linux by default. Statically pre-built binaries are still available.
  - Turns out that glibc, even in static executables, will load NSS to support `getaddrinfo`. The ABIs are less stable than the main shared-library entry-points potentially leading to crashes with "floating point exception" (e.g. Arch).
  - Note that linking statically against MUSL is currently not an option due to Deno's pre-built V8 requiring glibc.
  - Note further that we were already linking dynamically for other OSs due to lack of stable syscall ABIs.
- Update dependencies

## v0.16.3

- Admin settings UI: simplify forms by removing explicit empty string handling, better placeholders, add missing change email templates, ... .
- Internal: add a generalized WriteQuery abstraction tyding up query building but also in preparation of a potential batched transaction feature.
- Update Rust toolchain from 1.86 to latest stable 1.89.
- Update dependencies

## v0.16.2

- Fix regression allowing record updates to alter the primary key. In rare setups, this could be abused. For example, a primary key column referencing `_user(id)` would have allowed users to sign their records over to other users. Now covered by tests.
- Break up input (lazy-)params into separate insert & update params to simplify the individual paths.
- Update dependencies.

## v0.16.1

- Fix cleanup of pending file deletions and add comprehensive tests.
- Override default `--version` flag to yield version tag rather than crates version.
- Cleanup repo: all Rust code is now in a crates sub-directory.
- Update dependencies.

## v0.16.0

- Less magic: parse JSON/Form requests more strictly during record creation/updates. Previously, `{ "int_col": "1" }` would be parsed to `{ "int_col": 1 }`. If you were using type-safe, generated bindings or properly typed your requests, this change will not affect you.
- Rename auth query parameter `?redirect_to=` to `?redirect_uri=` in compliance with RFC 6749. I you are explicitly passing `?redirect_to=`, please update all references.
- Add "Send Test Email" feature to the admin UI.
- Reactively rebuild Record API configuration when database schemas change.
- Update dependencies.

## v0.15.13

- Record sequence of operations when altering table schemas to enable non-destructive column changes.
- Make columns in CreateAlterTable form append-only.
- Chore: clean up foreign key expansion code for Record APIs.
- Update dependencies.

## v0.15.12

- Rebuild Record API config on schema alterations.
- Parse strings more leniently to JSON: all integer, all real types.
- Accept a wider range of primary key values to support more complex `VIEW`s.
- Fix parsing of explicit "null" column constraint.
- Admin UI: clear dirty state on Record API submit and hide `STRICT`ness alteration in alter table form.
- Update dependencies.

## v0.15.11

- Make TypeScript `query`/`execute` database APIs type-safe.
- Consistently serialize/deserialize JS blobs as `{ blob: <urlSafeB64(bytes)> }`.
- Update dependencies.

## v0.15.10

- Significantly broaden the applicability of `VIEW`s for Record APIs.
  - Top-level `GROUP BY` clause can be used to explicitly define a key allowing arbitrary joins.
  - Also allow a broader set of expressions and joins in the general case.
  - Infer the type of computed columns for some simple built-in aggregations, e.g. MIN, MAX.
- Update dependencies.

## v0.15.9

- Auth settings UI:
  - Add explicit _Redirect URI_ for registration with external provider as suggested by @eugenefil.
  - Tidy up walls of help text.
  - Avoid rendering empty errors.
- Fix Gitlab OAuth provider, thanks @eugenefil.
- Unify input parameters and validation between password and OAuth login.
- Many more auth tests.
- Update dependencies.

## v0.15.8

- Support custom URI schmes for auth redirects like `my-app://callback`.
- Stop auto-redirect for signed in users if explicit `?redirect_to` is provided.
- Fix benign ghost session when logging in using PKCE.
- Fix param propagation for OAuth login flow.
- Many more tests and better documentation.
- Minor: clean up the repository of clients.
- Update dependencies.

## v0.15.7

- Add an official Golang client for TrailBase 🎉.
- Support simple sub-queries in `VIEW`-based APIs.
- Fix total `count` for `VIEW`-based APIs.
- Update Rust & JS dependencies.

## v0.15.6

- Make "Dry Run" button available for all table and index schema changes.
- Automatically re-render data table after changing the schema.
- Fix: column preset buttons and add a UUIDv4 preset.
- Update JS and Rust dependencies.

## v0.15.5

- Check out the [TanStack/db](https://github.com/TanStack/db) sync engine, which now officially supports TrailBase 🥳.
- Prevent schema alterations from invalidating Record API configuration, fix up renamed table and error when dropping referenced columns.
- Add dry-run to all the schema altering admin handlers.
- Fix: add missing symlink to assets.
- Update dependencies.

## v0.15.4

- Stricter JSON conversion and better errors for record APIs.
- Automatically fix-up record API config on table renames.
- Update dependencies.

## v0.15.3

- Fix parsing of `VIEW` definitions with elided aliases and handle `VIEW` parsing errors more gracefully.
- Fix `SIGHUP` migration/config order to match startup.
- Fix references in auth Emails.
- Documentation improvements: early schema migration doc, multi-API in UI, use of PKCE, etc.
- Update Rust & JS dependencies.

## v0.15.2

- Fix change email email form. Thanks @eugenefil!
- Admin UI: fix vertical scrolling in SplitView on small screens.
- Update dependencies.

## v0.15.1

- Re-apply migrations on SIGHUP. This allows applying schema changes w/o having to restart TrailBase.
- Polish: add copy&paste to log/user id and vertically align tooltips.
- Add a callout to the SQL editor warning users of schema skew when not using migrations.
- Minor: fix broken documentation link in dashboard.
- Update dependencies.

## v0.15.0

- Overhaul `user` and `admin` CLI commands to additionally support: change email, delete user, set verify status, invalidate sessions.
  - Users can now be referenced both by UUID (string and base64 encoded) and email addresses.
- With the extended CLI surface, disallow update/delete of admin users from the UI, thus reducing the potential for abuse. Priviledge now lies with sys-admins.
- Support install using a one-liner: `curl -sSL https://raw.githubusercontent.com/trailbaseio/trailbase/main/install.sh | bash` in addition to docker.
  - Also overhauled the "getting started" guide making it clearer how easy it is to install vanilla TrailBase and that docker is entirely optional.
- Fix record filtering when `[$is]=(!NULL|NULL)` is the only filter.
- Minor: reduce info log spam.

## v0.14.8

- Add an `$is` operator for filtering records. Allows filltering by `NULL` and `!NULL`.
- Add optional `--public-url` CLI argument and push responsibility of handling absent `site_url` down the stack to OAuth and Email components for more appropriate fallbacks.
- Documentation fixes around getting started with docker and relations.
- Update dependencies.

## v0.14.7

- Allow setting up multiple record APIs per `TABLE` or `VIEW` from with the admin UI. Previously, this was only available manually.
- User-configurable per-API limits on listing records, see config.
- Many small admin UI tweaks.
- Update Rust & JS dependencies.

## v0.14.6

- Render OAuth login options on the server rather than the client. Reduces the
  need for client JS.
- Fix `&redirect_to=` propagation for OAuth login.
- Denormalize `axum_extra::protobuf` and update Rust deps.

## v0.14.5

- Fix `&redirect_to=` propagation for password-based login.

## v0.14.4

- Improve OpenAPI definitions.
- Fix multi-row selection in Admin UI's table view.
- Update dependencies.

## v0.14.3

- Fix issue with listing records from `VIEW`s. We cannot rely on `_rowid_`,
  thus fall back to `OFFSET`. In the future we may add cursoring back for
  `VIEW`s that include a cursorable PK column.
- Minor: add an `limit=5` query parameter to list example queries.

## v0.14.2

- OpenAPI:
  - Include OpenAPI spec output into default builds with only Swagger UI behind
    a "swagger" feature flag.
  - Replace `--port` with `--address` for Swagger UI.
  - Add OpenAPI auth output to docs.
- Update dependencies.

## v0.14.1

- Admin UI:
  - Fix Record API id constraints to allow any UUID after v0.14.0 update.
  - Add more curl examples for record create/update/delete.
  - Fix and improve default value construction.
- More permissive default CORS, allow any headers.
- Update JS dependencies.

## v0.14.0

- Allow truly random UUIDv4 record IDs by relying on AES encrypted `_rowid_`s
  for cursors. UUIDv7, while great, has the problem of leaking creation-time.
  - Note: the encryption key for cursors is tied to the instance lifetime, i.e.
    they cannot be used across instance restarts (at least for now).
- Move user ids to UUIDv4 by default to avoid leaking user creation-time.
  - A bundled schema migration will update the PK to allow any UUID type, this is
    mostly to allow for existing users with UUIDv7 ids to continue to exist.
  - We expect this change to be transparent for most users but may break you,
    if you're relying on user ids being of the UUIDv7. Sorry, we felt this was an
    important change and wanted to rip off the band-aid. If you're having issues
    and are unsure on how to address them, please reach out and we'll help.

## v0.13.3

- Improve RecordAPI settings sheet with tabs, help text, and curl examples.
- Fix state update on key-stroke for input fields in admin UI.
- Minor: inline filter examples.
- Update Rust dependencies.

## v0.13.2

- Fix Admin-UI login form reactivity issue.

## v0.13.1

- Fix index names in admin UI.
- Update JS & Rust dependencies.

## v0.13.0

- Improve authentication and avatar handling (breaking).
  - Remove avatar handling dependence on _special_ `_user_avatar` record API by introducing dedicaed APIs.
    This is a breaking change to the semantics of `/api/auth/v1/avatar*`, which
    affects users of said APIs or `client.avatarUrl()`. Make sure to update to
    the latest JS/TS client (v0.6).
  - We also recommend removing the `_user_avatar` API definition from your `<traildepot>/config.textproto`.
    It's inconsequential to have but no longer needed.
  - Further, `/api/auth/v1/status` will now also refresh tokens thus not only
    validating the auth token but also the session. The JS/TS client uses this to
    asynchronously validate provided tokens.
  - Allow deletion of avatars on `/_/auth/profile`. Also adopt nanostores to
    manage client/user state on the profiles page.
  - Add avatars back to admin UI.
  - Document auth token lifecycle expectations when persisting tokens.
- Update dependencies.

## v0.12.3

- Fix row insertion/update in admin dashboard.
- Fall back to downloading JS deps during build when missing. This helps with vendoring TrailBase for framework use-cases.
- Update dependencies.

## v0.12.2

- Fix unchecked null assertion in admin auth dashboard.
- Update JS dependencies.

## v0.12.1

- Use fully-qualified databases everywhere. Preparation for multi-DB.
- Support for for Maxmind's city-geoip DB and command line to specificy custom
  DB locations.
- Explicitly parse cursor based on schema.
- Show command line in admin dashboard
- Improve admin dash's state management .
- Internal: Reduce dependence on vendored crates.
- Update dependencies including latest version of SQLite.

## v0.12.0

- Overhaul list API filters to allow for nested, complex expressions. The query
  parameters change making is a **breaking** change. Users will need to update
  their client libraries.
  - All clients have been updated to both: support the new syntax and help in
    the construction of nested filters.
  - For raw HTTP users, the filter format went from `col[ne]=val` to
    `filter[col][$ne]=val` following QS conventions.
  - For example, exluding a range of values `[v_min, v_max]`:
    `?filter[$or][0][col][$gt]=v_max&filter[$or][1][col][$lt]=v_min`.
- A new client implementation for the Swift language.
- Show release version in the admin dashboard and link to release page.
- Update dependencies.

## v0.11.5

- Improved admin SQL editor: save dialog and pending change indication.
- Fix short-cut links on dashboard landing page.
- Update dependencies.

## v0.11.4

- Replaced Mermaid-based schema renderer with x6.
- Fix admin UI create-table regression.

## v0.11.3

- Add simple schema visualizer to admin UI. This is a starting point.
- Configurable password policies: length, characters, ...
- Turn admin UI's landing page into more of a dashboard, i.e. provide some
  quick numbers on DB size, num users, ...
- Some small fixes and internal cleanups, e.g. preserve `redirect_to`, simplify
  state management, ...
- Update dependencies.

## v0.11.2

- Rate-limit failed login attempts to protect against brute force.
- Add option to disallow password-based sign-up.
- Fix 404 content-type.
- Fix escaping of hidden form state in auth UI after moving to askama templates.
- Update dependencies.

## v0.11.1

- While JS transactions are waiting for a DB lock, periodically yield back to
  the event loop to allow it to make progress.
- Allow using foreign key expansion on record APIs backed by `VIEW`s.

## v0.11.0

- Support SQLite transactions from JS/TS, e.g.:

  ```ts
  import { transaction, Transaction } from "../trailbase.js";

  await transaction((tx: Transaction) => {
    tx.execute("INSERT INTO 'table0' (v) VALUES (5)", []);
    tx.execute("INSERT INTO 'table1' (v) VALUES (23)", []);
    tx.commit();
  });
  ```

  WARN: This will block the event-loop until a lock on the underlying database
  connection can be acquired. This may become a bottleneck if there's a lot of
  write congestion. The API is already async to transparently update the
  implementation in the future.

- Update rusqlite to v0.35. On of the major changes is that rusqlite will no
  longer quietly ignore statements beyond the first. This makes a lot of sense
  but is a breaking change, if you were previously relying on this this odd
  behavior.
- Overhaul JS runtime integration: separate crate, unified execution model, use
  kanal and more tests.
- Added `trailbase_sqlite::Connection::write_lock()` API to get a lock on the
  underlying connection to support JS transactions in a way that is compatible
  with realtime subscriptions w/o blocking the SQLite writer thread for
  extended periods of time while deferring control to JS.
- Fix benign double slash in some urls.
- Minor: improve internal migration writer.
- Update other dependencies.

## v0.10.1

- Further refine SQLite execution model.
  - Previously reader/writer queues were processed independently. That's great
    for naive benchmarks but not ideal for more real-world, mixed workloads.
  - Use an in-process RwLock to orchestrate access avoiding file-lock congestion.
- Improve Record listing:
  - Support `?offset=N` based pagination. Cursor will always be more efficient when applicable.
  - Updated all the clients to support offset.
  - Error on in-applicable cursors.
  - Error on user-provided `?limit=N`s exceeding the hard limit.
- Fix corner cases for not properly escaped and not fully-qualified filter column names.
- Update dependencies.

## v0.10.0

- Finer-grained access control over exposed APIs on a per-column basis:
  - Columns can be explicitly excluded via a `excluded_columns` config option
    rendering them inaccessible for both: reads and writes. This is different
    from columns prefixed with "\_", which are only hidden from read operations.
  - A new `_REQ_FIELDS_` table is availble during access checks for `UPDATE` and
    `CREATE` endpoints allowing to check for field presence in requests, e.g.
    `'field' IN _REQ_FIELDS_`. A field in `_REQ`\_ will be `NULL` whether it was
    absent or explicitly passed as `null`.
- Early message queue work (WIP).
- Updated dependencies.

## v0.9.4

- Overhaul insert/update row/record form:
  - Integer primary keys are nullable.
  - Explicit nullability for numbers.
  - Ignore defaults for update path.
  - Don't pre-fill defaults.
- Install SIGHUP handler for config reload.
- Update to Rust edition 2024.
- Update dependencies.

## v0.9.3

- Custom JSON stdout request logger to have a stable format as opposed to
  depending on the span/event structure, which is an implementation detail.
- Show response timestamps in dashboard with millisecond resolution.
- Log response timestamp explicitly.
- Improve logs writer performance: no transaction needed, improved statement
  caching.
- Improve incremental release build times by ~70% switching from "fat" to "thin" LTO.
- Update dependencies.

## v0.9.2

- Overhaul SQLite execution model to allow for parallel reads. This should help
  reduce latency long-tail with slow queries.
  - And add more benchmarks.
- Log request/response logs to stdout in JSON format.
- Always re-create traildepot/.gitignore. Previously gated on creating the root
  path, which was never the case for docker users.
- Update dependencies.

## v0.9.1

- Consistently expanded JSON schemas for specific APIs everywhere (UI & CLI).
- Improved foreign table id lookup during schema evaluation.
- Stricter SQL validation in admin UI.
- Break up sqlite and core creates into two additional crates: schema & assets.
- Update dependencies.

## v0.9.0

- Performance:
  - Read and write latency improved both by ~30% 🔥.
  - Memory footprint dropped by ~20% in our insert benchmarks.
  - Build narrower INSERT queries.
  - Use more cached statements.
- Overhaul object-store/S3 file life-cycle/cleanup.
  - Use triggers + persistent deletion log.
  - Retry cleanups on transient object store isues.
  - Fix issue with zombie files on UPSERTs.
- Decouple record APIs form underlying TABLE/VIEW schemas.
- Fix leaky abstractions by pushing tracing initialization into server
  initialization and more strictly separate from logging.
- Update dependencies.

## v0.8.4

- Add a `?loginMessage=` query parameter to admin login page.
- Move query construction for more complex queries to askama templates and add more tests.
- Move subscription-access query construction from hook-time to RecordApi build-time.
- Use askama for auth UI.

## v0.8.3

- Support more SQL constructs:
  - Conflict clause in table and column unique constraints.
  - FK triggers in column constraints.
  - CHECK table constraints.
- Fix: pagination cursors in list queries for arbitrary PKs.
- Sanitize expand and order column names in list queries.
- Update dependencies.

## v0.8.2

- Quote table/index/column names during "CREATE TABLE/INDEX" parsing and construction.
- Improve auth UI: more consistent shadcn styling and explicit tab orders.
- UUID decode sqlite extension and more consistent extension names.
- Update deps.

## v0.8.1

- Derive job id in native code for JS/TS jobs.
- Fix conflict resolution selector in admin UI's API settings.
- Fix primary key card collapsing in create table form.

## v0.8.0

- Add support for periodic cron jobs:
  - Add dashboard to admin UI to inspect, configure and trigger cron jobs.
  - Users can register their own cron jobs from the JS runtime.
  - Replace internal periodic tasks with cron jobs to increase configurability,
    discoverabilty, and avoid drift.
  - BREAKING: removed `backup_interval_sec` from proto config. When explicitly specified,
    users will need to remove it from their `<traildepot>/config.textproto` and set an
    appropriate cron schedule instead.

## v0.7.3

- Cleanup logs DB schema and log ids of authenticated users.
- Allow setting the name and INTEGER type for PKs in create table form.
- Fix reactivity for FK settings in create/alter table forms.
- Add confirmation dialog for user deletions.
- Limit mutations in `--demo` mode.
  - Dedicated admin delete user endpoint.
- Unified parameter building for listing records, users and logs.
- Cleanup backend auth code and query API.
- Update dependencies including rusqlite.

## v0.7.2

- Fix and test OpenId Connect (OIDC) integration.
- Audit and remove unwraps.
- Fix auth img-src CSP for external avatars and dev instances.

## v0.7.1

- Add generic OIDC provider. Can currently only be configured in config. Admin UI integration pending.
- Add --demo mode to protect PII in demo setup.
- Improve secrets redaction/merging.

## v0.7.0

- Schema-aware auto-completion in SQL editor.
- Allow UUID text-encoded 16byte blobs as record ids and in filters during record listing.
- Redact secrets in admin APIs/UI to reduce surface for potential leaks.
- Polish auth/admin UI with image assets for external auth providers like discord, gitlab, ... .
- Permissive `img-src` CSP in auth UI to allow displaying avatars from external auth providers.

## v0.6.8

- Fix client-side merging of fetch arguments including credentials.
- Improved auth UI styling.

## v0.6.7

- Improve token life cycle for JS/TS clients including admin dash.

## v0.6.6

- Add a dialog to avoid accidentally discarding unsaved changes in the SQL editor.
- Polish UI: animate buttons, consistent refresh, avoid logs timestamp overflow.
- Update Rust and JS deps.

## v0.6.5

- Fix routing issues with auth UI.
- Redirect /login to /profile on already logged in.
- Redirect /register to /login?alert= on success.
- Persist execution result in Admin SQL editor.
- Address linter issues.
- Update dependencies.

## v0.6.4

- Add undo history to query editor and improve error handling.
- Cosmetic improvements of Admin UI like more consistency, more accessible buttons, ...
- Indicate foreign keys in table headers.
- Turn table into a route parameter and simplify state management.
- Fix hidden table UI inconsistency.
- Fix input validation issues in form UI.
- Limit cell height in Table UI.

## v0.6.3

- Allow downloading JSON schemas from the UI for all modes: Insert, Update, Select.
- Add some more UI polish: tooltips, optics, and tweaks.
- Improve UI type-safety

## v0.6.2

- Update to address broken vite-plugin-solid: https://github.com/solidjs/vite-plugin-solid/pull/195.

## v0.6.1

- Fix config handling in the UI.
- Improve form handling in the UI.
- Few minor UI fixes & cleanups.
- Update dependencies.

## v0.6.0

- Support foreign record expansion. If a record API is configured allow
  expansion of specific foreign key columns, clients can request to expand the
  parent record into the JSON response of RecordApi `read` and `list`. This is
  also reflected in the JSON schema and warrants a major version update.
  Updates to all the client packages have already been pushed out.
- Support for bulk record creation. This is particularly useful when
  transactional consistency is advisable, e.g. creating a large set of M:N
  dependencies.
- Record subscriptions now have to be explicitly enabled in the
  admin-UI/configuration
- Simplify PNPM workspace setup, i.e. get rid of nesting.
- Fixed rustc_tools_util upstream, thus drop vendored version.
- Reduce logs noise.
- Update dependencies.

## v0.5.5

- Fix build metadata release channel and include compiler version.
- Admin UI: Avoid triggering table's onClick action on text selection.
- Update deps.

## v0.5.4

- Add a `?count=true` query parameter to RecordApi.list to fetch the total
  number of entries.
- Return error on invalid list queries rather than skipping over them.
- Address Admin UI issues:
- Stale config after altering schema or dropping table.
- Out-of-sync filter bar value.
- Reset filter when switching tables.
- Hide "sqlite\_" internal tables in Admin UI.

## v0.5.3

- Built-in TLS support.
- Add "server info" to the admin dashboard, e.g. including build commit hash.
- Update deps.

## v0.5.2

- Add file-system APIs to JS/TS runtime to facility accessing resources, e.g.
  templates for SSR (see example/colab-clicker-ssr).
- Add a timeout to graceful shutdown to deal with long-lived streaming connections.
- Allow short-cutting above timeout by pressing a Ctrl+C second time.

## v0.5.1

- Update SQLite from 3.46.1 to 3.48.0.

## v0.5.0

- Breaking change: RecordApi.list now nests records in a parent structure to
  include cursor now and be extensible for the future.
- Update all the client libraries to expect a ListResponse.

## v0.4.1

Minor update:

- Fix issue with delete table potentially invalidating config due to stale RecordAPI entries.
- Update dependencies.

## v0.4.0

Added an early version of Record change subscriptions, a.k.a. realtime, APIs.
Users can now subscribe to an entire API/table or specific record to listen for
changes: insertions, updates, deletions (see client tests, docs are TBD).

## v0.3.4

- Update Axum major version to v0.8.
- Major overhaul of project structure to allow for releasing crates.

## v0.3.3

- Pre-built Windows binary.

## v0.3.2

- Move record API access query construction to RecordApi construction time.
- Cache auth queries
- Some tweaks and hooks API for trailbase_sqlite::Connection.
- Remove sqlite-loadable and replace with rusqlite functions.
- Reduce allocations.

## v0.3.1

- Fix client-ip logging.
- Wire request-type into logs

## v0.3.0

A foundational overhaul of SQLite's integration and orchestration. This will
unlock more features in the future and already improves performance.
Write performance roughly doubled and read latencies are are down by about two
thirds to sub-milliseconds 🏃:

- Replaced the libsql rust bindings with rusqlite and the libsql fork of SQLite
  with vanilla SQLite.
- The bindings specifically are sub-par as witnessed by libsql-server itself
  using a forked rusqlite.
- Besides some missing APIs like `update_hooks`, which we require for realtime
  APIs in the future, the implemented execution model is not ideal for
  high-concurrency.
- The libsql fork is also slowly getting more and more outdated missing out on
  recent SQLite development.
- The idea of a more inclusive SQLite is great but the manifesto hasn't yet
  manifested itself. It seems the owners are currently focused on
  libsql-server and another fork called limbo. Time will tell, we can always
  revisit.

Other breaking changes:

- Removed Query APIs in favor of JS/TS APIs, which were added in v0.2. The JS
  runtime is a lot more versatile and provides general I/O. Moreover, query APIs
  weren't very integrated yet, for one they were missing an Admin UI. We would
  rather spent the effort on realtime APIs instead.
  If you have an existing configuration, you need to strip the `query_apis`
  top-level field to satisfy the textproto parser. We could have left the
  field as deprecated but since there aren't any users yet, might as well...

Other changes:

- Replaced libsql's vector search with sqlite-vec.
- Reduced logging overhead.

## v0.2.6

- Type JSON more strictly.
- Fix input validation for nullable columns in the insert/edit row Admin UI form.

## v0.2.5

- Addresses issues reported by reddit user _qwacko_ 🙏
  - Fix serialization of foreign key column options.
  - Fix deserialization of TableIndex.
  - Admin UI: Show all tables, including hidden ones, in create-table-form's
    drop down for column foreign-keys.

## v0.2.4

- Allow configuring S3 compatible storage backend for file uploads.

## v0.2.3

- Interleaving of multiple HTTP requests into busy v8 isolates/workers.
- JS runtime:
  - add `addPeriodicCallback` function to register periodic tasks that
    executes on a single worker/isolate.
  - Constrain method TS argument type (`MethodType`).

## v0.2.2

- Enable "web" APIs in JS runtime.
- Add "Facebook" and "Microsoft" OAuth providers.

## v0.2.1

- Allow setting the number V8 isolates (i.e. JS runtime threads) via
  `--js-runtime-threads`.

## v0.2.0

- Add JS/ES6/TS scripting support based on speedy V8-engine and rustyscript runtime.
  - Enables the registration of custom HTML end-points
  - Provides database access.
  - In the future we'd like to add more life-cycles (e.g. scheduled
    operations).
  - In our [micro-benchmarks](https://trailbase.io/reference/benchmarks/) V8
    was about 45x faster than goja.
- Added official C#/.NET client. Can be used with MAUI for cross-platform
  mobile and desktop development.

## v0.1.1

- Changed license to OSI-approved weak-copyleft OSL-3.0.
- Add GitHub action workflow to automatically build and publish binary releases
  for Linux adm64 as well as MacOS intel and apple arm silicone.
- Add support for geoip database to map client-ips to countries and draw a world map.

## v0.1.0

- Initial release.
