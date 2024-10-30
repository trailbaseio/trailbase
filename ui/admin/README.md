# TrailBase Admin Dashboard UI

## Codegen proto code

We're using https://github.com/stephenh/ts-proto#usage for typescript generation.

    $ pnpm run proto

Make sure to install:

 * protobuf-compiler, for protoc
 * libprotobuf-dev, for meta files such as descriptor.proto.

## Codegen Rust-TypeScript bindings

They are currently created on `cargo test` and copied to `/bindings` on `cargo
build` where they're being picked up.
