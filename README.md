<p align="center">
  <a href="https://trailbase.io" target="_blank">
    <picture>
      <img alt="TrailBase logo" width="150" src="https://raw.githubusercontent.com/trailbaseio/trailbase/refs/heads/main/assets/logo.svg" />
    </picture>
  </a>
</p>

<p align="center">
  A <a href="https://trailbase.io/reference/benchmarks/">blazingly</a> fast,
  single-file, open-source application server with type-safe APIs, built-in
  JS/ES6/TS Runtime, Auth, and Admin UI built on Rust+SQLite+V8.
<p>

<p align="center">
  <a href="https://github.com/trailbaseio/trailbase/stargazers/">
    <img src="https://img.shields.io/github/stars/trailbaseio/trailbase?style=social&label=Star" />
  </a>
  <a href="https://github.com/trailbaseio/trailbase/actions?query=branch%3Amain">
    <img src="https://github.com/trailbaseio/trailbase/actions/workflows/test.yml/badge.svg?branch=main" alt="Build Status">
  </a>
  <a href="https://github.com/trailbaseio/trailbase/blob/main/LICENSE">
    <img src="https://img.shields.io/badge/license-OSL_3.0-blue" alt="License - OSL 3.0">
  </a>
  <a href="https://trailbase.io/reference/roadmap/">
    <img src="https://img.shields.io/badge/status-alpha-orange" alt="Status - Alpha">
  </a>
</p>

# TrailBase

<p align="center">
  <a href="https://demo.trailbase.io/_/admin" target="_blank">
    <picture>
      <img alt="Admin UI" width="512" src="https://raw.githubusercontent.com/trailbaseio/trailbase/refs/heads/main/docs/src/assets/screenshot.webp" />
    </picture>
  </a>
</p>

<p align="center">
  Try the <a href="https://demo.trailbase.io/_/admin" target="_blank">demo</a> online - Email: <em>admin@localhost</em>, password: <em>secret</em>.
</p>

For more context, documentation, and an online live demo, check out our website
[trailbase.io](https://trailbase.io).
Questions? Thoughts? Check out the [FAQ](https://trailbase.io/reference/faq/)
on our website or reach out.
If you like TrailBase or its prospect, consider leaving a ⭐🙏.

## Project Structure & Releases

This repository contains all components that make up TrailBase including client
libraries, tests, documentation and examples.
Only the [benchmarks](https://github.com/trailbaseio/trailbase-benchmark) are
kept separately due to their external dependencies.

Pre-built static binaries are available as [GitHub
releases](https://github.com/trailbaseio/trailbase/releases/) for Linux an
MacOS.

Moreover, client packages and containers are available via:

- [Docker](https://hub.docker.com/r/trailbase/trailbase)
- [JavaScript/Typescript client](https://www.npmjs.com/package/trailbase)
- [Dart/Flutter client](https://pub.dev/packages/trailbase)
- [C#/.Net](https://www.nuget.org/packages/TrailBase/)
- [Python](https://pypi.org/project/trailbase/)

## Running

You can run pre-built TrailBase either by downloading the latest
[release](https://github.com/trailbaseio/trailbase/releases/) and running

```bash
$ ./trail run
```

or using docker:

```bash
$ mkdir traildepot
$ alias trail="docker run -p 4000:4000 --mount type=bind,source=$PWD/traildepot,target=/app/traildepot trailbase/trailbase /app/trail"
$ trail run
```

. Run `trail --help` to get a full list of commands. If you don't want to rely
on pre-built binaries, TrailBase is easy to build yourself, see below.

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

Contributions are very much appreciated 🙏. For anything beyond bug fixes,
let's quickly chat to see how a proposal fits into the overall roadmap and
avoid any surprises.

We're not sure yet what the best setup or exact license is for compatibility
between OSL-3.0 and more popular licenses or use as a framework.
So we'd ask you to sign a simple CLA that retains your copyright, ensures that
TrailBase will continue to forever be freely available under an OSI-approved
copyleft license, while allowing for some flexibility and sub-licensing as
established by much larger, successful projects such as Grafana or Element.

## License

TrailBase is free software under the terms of the [OSL-3.0](LICENSE).

We chose this license over more popular, similar copyleft licenses such as
AGPLv3 due to its narrower definition of derivative work that only covers
modifications to TrailBase itself. This is similar to GPL's classpath exception
or LGPL's linkage exception allowing the use of TrailBase as a framework
without inflicting licensing requirements on original work layered on top.
That said, we ain't lawyers. The author of the license provides a more
thorough [explanation](https://rosenlaw.com/OSL3.0-explained.htm).
If you have any concerns or advice for us, please reach out.

If you require an
[exception](https://www.gnu.org/philosophy/selling-exceptions.html), reach out
to contact@trailbase.io.
