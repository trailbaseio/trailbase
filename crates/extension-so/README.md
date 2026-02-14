# TrailBase standalone SQLite extension

TrailBase relies on a hand-full of extensions, e.g. to assert that a `PRIMARY
KEY` is in a certain `UUID` format. To avoid lock-in, i.e. make it easier
for folks to migrate into and away from TrailBase, we keep the extensions
minimal and make them available externally. This crate facilitates building
said standalone extensions.

## Build & Usage

In the repository root, simply run:

```sh
cargo build -p trailbase-extension-so --release
```

This will produce a shared-object, which you can simply load:

```sh
sqlite3

sqlite> .load target/release/libtrailbase.so
sqlite> SELECT uuid_text(uuid_v4());
67136039-2981-497c-96ca-ae5c06a3cb98
```
