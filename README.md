# TrailBase

A blazingly fast, single-file, and open-source server for your application with
type-safe restful APIs, auth, admin dashboard, etc.

For more context, documentation, and an online live demo, check out our website
[trailbase.io](https://trailbase.io).

## FAQ

Check out our [website](https://trailbase.io/reference/faq/).

## Project Structure

This repository contains all components that make up TrailBase, as well as
tests, documentation and examples.
Only our [benchmarks](https://github.com/trailbaseio/trailbase-benchmark) are
kept separately due to their external dependencies.

## Building

If you have all the necessary build dependencies (rust, nodejs, pnpm, ...)
installed, you can simply build TrailBase by running:

```bash
$ git submodule update --init --recursive
$ cargo build
```

Alternatively, you can build with docker:

```bash
$ git submodule update --init --recursive
$ docker build . -t trailbase
```

## Contributing

Contributions are very welcome, let's just talk upfront to see how a proposal
fits into the overall roadmap and avoid any surprises.
