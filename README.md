# TrailBase

A [blazingly](https://trailbase.io/reference/benchmarks/) fast, single-file,
and open-source application-base built on top of Rust and SQLite (libsql).
It ships with type-safe restful APIs, an authentication system, an admin
dashboard and more out of the box.

For more context, documentation, and an online live demo, check out our website
[trailbase.io](https://trailbase.io).
Questions? Thoughts? Check out the [FAQ](https://trailbase.io/reference/faq/)
on our website or reach out.

## Project Structure & Releases

This repository contains all components that make up TrailBase including client
libraries, tests, documentation and examples.
Only the [benchmarks](https://github.com/trailbaseio/trailbase-benchmark) are
kept separately due to their external dependencies.

Packages and pre-built binaries are available via:

- [Docker](https://hub.docker.com/r/trailbase/trailbase)
- [JavaScript/Typescript client](https://www.npmjs.com/package/trailbase)
- [Dart/Flutter client](https://pub.dev/packages/trailbase)

Pre-built static binaries are also available as
[GitHub releases](https://github.com/trailbaseio/trailbase/releases/).
At the moment that is builds for `Linux x86_64` only until we get our
cross-platform build infrastructure set up.
In the meantime, you can use docker or try building it yourself. We have built
TrailBase successfully on Mac and Apple silicone, see instructions below.

## Building

If you have all the necessary dependencies (rust, nodejs, pnpm, ...) installed,
you can build TrailBase simply by running:

```bash
$ git submodule update --init --recursive
$ cargo build --release
```

To build fully static binaries on Linux (et al):

```bash
$ RUSTFLAGS="-C target-feature=+crt-static" cargo build --target x86_64-unknown-linux-gnu --release
```

Alternatively, if you want a container or don't have to deal with dependencies,
you can build using docker:

```bash
$ git submodule update --init --recursive
$ docker build . -t trailbase
```

## Contributing

Contributions are very welcome 🙏. Let's talk to see how a proposal fits into
the overall roadmap and avoid surprises.

## License

TrailBase is free software under the terms of the [AGPLv3](LICENSE.md). If you
require an [exception](https://www.gnu.org/philosophy/selling-exceptions.html),
reach out to contact@trailbase.io.
