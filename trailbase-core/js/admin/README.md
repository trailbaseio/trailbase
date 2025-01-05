# TrailBase Admin Dashboard UI

This directory contains TrailBase's admin dashboard UI. It is an SPA, i.e. it's
static content embedded into the TrailBase binary that executes in the browser
using TrailBase's privileged admin APIs.

For development one case use the vite dev-server with hot module reload. First
start the TrailBase binary in dev mode (permissive CORS and cookie policies):

```bash
$ cargo run -- --data-dir client/testfixture  run --dev
```

and then vite:

```bash
$ pnpm run dev
```

## Protobuf codegen

We're using [ts-proto](https://github.com/stephenh/ts-proto#usage) for
protobuf code generation. Run

```bash
$ pnpm run proto
```

, which requires the following system dependencies:

 * **protoc**,  e.g. via the `protobuf-compiler` Debian/Ubuntu package
 * **descriptor.proto**, e.g. via the `libprotobuf-dev` Debian/Ubuntu package.

## Rust-TypeScript codegen

The TypeScript bindings for the admin APIs are checked into the repository
under `/trailbase-core/bindings`. They're generated via `ts-rs` and written
every time the Rust tests execute, i.e. `cargo test`. They're checked in only
to avoid having Rust depend on TypesScript and TypeScript depend on Rust tests.
