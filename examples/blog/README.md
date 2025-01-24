# TrailBase Example: A Blog with Web and Flutter UIs

<p align="center">
  <picture align="center">
    <img
      height="420"
      src="screenshots/screenshot_web.png"
      alt="Screenshot Web"
    />
  </picture>

  <picture align="center">
    <img
      height="420"
      src="screenshots/screenshot_flutter.png"
      alt="Screenshot Flutter"
    />
  </picture>
</p>

The main goal of this example is to be easily digestible while show-casing many
of TrailBase's capabilities both for web and cross-platform Flutter:

* Bootstrapping the database including schemas and dummy content though migration.
* End-to-end type-safety through code-generated data models for TypeScript,
  Dart and many more based on JSON Schema.
* Builtin web authentication flow (including OAuth) on web and Flutter as well
  as a custom password-based login in Flutter.
* API authorization: world readable, user editable, and moderator manageable articles.
* Different API types:
 * Table and View-based APIs for custom user profiles associating users with a
   username and keep their email addresses private as well as associating
   articles with usernames.
 * Virtual-table-based query API to expose `is_editor` authorization.
* The web client illustrates two different styles: a consumer SPA and an
  HTML-only form-based authoring UI.

Default users:

 * (email: `admin@localhost`, password: `secret`) - access to admin dash.
 * (email: `editor@localhost`, password: `secret`) - permission to write and alter blog posts.

## Directory Structure

```
.
├── Caddyfile           # Example reverse proxy for TLS termination
├── Dockerfile          # Example for bundling web app
├── docker-compose.yml  # Example setup with reverse proxy
├── flutter             #
│   ├── lib             # Flutter app lives here
│   └── ...             # Most other files a default cross-platform setup
├── Makefile            # Builds JSON schemas and coge-generates type definitions
├── schema              # Checked-in JSON schemas
├── traildepot          # Where TrailBase keeps its runtime data
│   ├── backups         # Periodic DB backups
│   ├── data            # Contains SQLite's DB and WAL
│   ├── migrations      # Bootstraps DB with schemas and dummy content
│   ├── secrets         # Nothing to see :)
│   └── uploads         # Local file uploads (will support S3 soon)
└── web
    ├── dist            # Built/packaged web app
    ├── src             # Web app lives here
    └── types           # Generated type definitions
    └── ...
```

## Instructions

Generally speaking, there are roughly 2.5 moving parts to run the example, i.e:
we have to build the web UI, start the TrailBase server, and optionally start
the Flutter app. Once you have `cargo`, `pnpm`, and `flutter` installed, you
can simply run:

```bash
# From within the blog examples base directory
$ cd $REPO/examples/blog

# build and bundle the web app:
$ pnpm --dir web build

# Start TrailBase:
cargo run --bin trail -- run --public web/dist

# Start Flutter app:
$ cd flutter
$ flutter run -d <Device, e.g.: Linux, Chrome, Mobile Emulator, ...>
```

You can also try the code generation:

```bash
# Optionally delete the checked-in JSON schemas and code first
$ make clean_types

# Genarate JSON Schema and codegen types from DB schema (this requires that
# you start TrailBase first to initialize the DB)
$ make --always-make types
```

## Reference

* The styling is based on: https://github.com/palmiak/pacamara-astro 🙏
