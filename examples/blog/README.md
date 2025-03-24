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

The blog example presents some of TrailBase's capabilities in an easily easily
digestible fashion:

* Building UI Apps both for web and cross-platform using Flutter.
* End-to-end type-safe APIs based on JSON schemas and code-generation
  supporting most popular languages.
* Authentication flows with social OAuth and password sign-in for web and Flutter.
* Authorization: world readable, user editable, and moderator manageable articles.
* Different API types:
  * Table and View-based APIs for custom user profiles associating users with a
    username to keep their email addresses private as well as associating
    articles with usernames.
  * Virtual-table-based query API to expose `is_editor` authorization.
* Migrations to bootstrap the database with schemas and dummy content.
* The web UI is implemented as a reader-side SPA and static HTML page for blog
  authors to demonstrate both styles.

## Getting Started

To get the blog up and running with self-signed SSL certificates in under 2
minutes, simply run:

```bash
cd examples/blog
docker compose up --build -d
```

Afterwards check out the blog at [http://localhost](http://localhost). You'll
be automatically forwarded to HTTPS and will need to accept the self-signed
certificate.
You can write new blog posts using the predefined user:

  * email: `editor@localhost`
  * password: `secret`

You can also check out the admin dashboard at
[http://localhost/_/admin](http://localhost/_/admin) using the predefined
admin:

  * email: `admin@localhost`
  * password: `secret`

For context, the above `docker compose` invocation started two services:

 * TrailBase itself hosting the web UI, and
 * a [Caddy](https://github.com/caddyserver/caddy) reverse-proxy to
   automatically terminate TLS using self-signed certificates demonstrating a
   production-ready setup.

To shut everything back down, simply run:

```bash
docker compose down
```

## Detailed Instructions

If you don't want to use the docker compose setup above, build from scratch, or
run the the Flutter app, only a few simple steps are needed.
If you have `cargo`, `pnpm`, and `flutter` installed, you can simply run:

```bash
# Build the Blog's web UI:
$ pnpm --dir web build

# Build and start TrailBase:
$ cargo run --bin trail -- run --public web/dist

# Build and start the Flutter app:
$ cd flutter
$ flutter run -d <Device, e.g.: Linux, Chrome, Mobile Emulator, ...>
```

In case you'd like to re-generate the language bindings for the type-safe APIs
or generate new bindings for a different language, check out the `Makefile` or
run:

```bash
$ make --always-make types
```

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

## Reference

* The styling is based on: [palmiak/pacamara-astro](https://github.com/palmiak/pacamara-astro) 🙏
