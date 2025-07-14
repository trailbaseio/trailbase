---
title: Comparisons
---

## TrailBase vs. PocketBase

Firstly, PocketBase is amazing! It paved the way for single-executable
application bases, is incredibly easy-to-use, and a polished
experience. Gani, the person behind it, is a mad scientist 🙏.

From a distance PocketBase and TrailBase are both single-executables providing
almost identical feature sets: REST APIs, realtime updates, authentication, file
storage, JavaScript (JS) runtimes, admin dashboard..., all on top of SQLite.

For the sake of this comparison, we'll dive a little deeper to have a closer
look at their differences both technically and philosophically.

### Goals

TrailBase was born out of admiration for PocketBase trying to move the needle
in a few areas:

- Less abstraction and embracing standards (SQL[^1], ES6 Node-like JS runtime,
  JWT, UUID) to not get in your way, avoid lock-in and making it easier to
  adopt TrailBase either fully or as piece-meal as well as getting rid of it if
  necessary.
- Untethered access to SQLite[^2] including features such as recursive CTEs,
  virtual tables and extensions, e.g., providing vector search and geoip out of
  the box[^3], unlocking more use-cases and standard solutions to well-known
  problems.
- Be lightweight and fast to rival in-process SQLite performance at least for
  higher-level languages.
- Be just as easy to self-host with aspirations to be even easier to manage a
  fleet of deployments across integration tests, development, and production.
  We'd also like to eventually tackle replication.
- Be simple, flexible and portable enough to support data analysis use-cases.
  Imagine a self-contained data science project that ships an interactive UI
  with vector search and custom JS extensions alongside its data.

### Differences

Beyond goals and intentions, let's look at some of the more practical differences:

#### Language & Performance

PocketBase being written in Go and TrailBase in Rust may be the most instantly
visible difference.
Preference aside, this will likely matter more to folks who want to use
either as a framework rather than the standalone binary or modifying the core.

In practice, both languages are solid, speedy options with rich ecosystems.
Though Rust's lack of a runtime and lower FFI overhead gives it the edge in
terms of performance.
Measuring we found that TrailBase's APIs are roughly [10x faster](/why-trailbase/benchmarks/).
This may sound like a lot but is the result of SQLite itself being extremely
fast meaning that even small overheads weigh heavily.

Independently, TrailBase choice of V8 as its JS runtime allows code to run
roughly 40x faster.

#### Framework Use

Both PocketBase and TrailBase allow customization using their built-in JS
runtimes. However, some users may want even more control and built their own
binaries using only bits and pieces.
This is what we refer to as library or framework use. For this use-case
language preference and prior experience of you and your team will likely matter
a lot more with PocketBase written in Go and TrailBase in Rust.

Framework use is something PocketBase has allowed for a long time and is really
great at.
TrailBase technically allows it too, but at this point it really feels more
like an afterthought while we focus on the standalone experience.
Expect TrailBase to improve significantly in this area.

#### Features

When we look more deeply into the seemingly identical features sets, many and
constantly evolving differences are starting to surface.
In lieu of enumerating them all, let's look at some examples.

Auth is an area where PocketBase's maturity clearly shows: while it uses
simpler session-based auth, as opposed to stateless JWT auth-tokens, it
supports multi-factor auth and a larger set of social OAuth providers.
This is an area where TrailBase needs to improve but maybe stateless tokens is
just what you're after 😅.

Despite being the new kid on the block, TrailBase has a few nifty tricks up its
sleeve:

- Language independent bindings via JSON-schema with strict type-safety
  being enforced from the client all the way to the database[^4].
- A more Node-like JS runtime with full ES6 support, built-in TypeScript
  transpilation, and V8 performance unlocking more of the JS ecosystem and enabling
  [server-side rendering (SSR)](https://github.com/trailbaseio/trailbase/tree/main/examples/collab-clicker-ssr)
  with any popular JS framework.
- Untethered access to SQLite with all its features and capabilities.
- A wider set of first-class client libraries beyond JS/TS and Dart, including
  C#, Python and Rust.
- Ships with a simple pre-built auth UI to get you started. You can always
  graduate to your own.
- Efficient and stable cursor-based pagination as opposed to `OFFSET`.
- An admin UI that "works" on small mobile screens 😅.

#### Contributing & Licensing

Both PocketBase and TrailBase are truly open-source: they accept contributions
and are distributed under [OSI-approved](https://opensource.org/licenses) licenses.
PocketBase is distributed under the permissive MIT license, while TrailBase
uses the OSL-3.0 copyleft license.
We chose this license over more popular, similar copyleft licenses such as
AGPLv3 due to its narrower definition of derivative work that only covers
modifications to TrailBase itself. This is similar to GPL's classpath or LGPL's
linkage exception allowing the use of TrailBase as a framework and JS runtime
without inflicting licensing requirements on your original work.

### Final Words

PocketBase is great and both PocketBase and TrailBase are constantly evolving
making it hard to give clear guidance on which to pick when.
If you can afford the luxury, I'd recommend to give them both a quick spin.
After all they're both incredibly quick and easy to deploy.

In the end, if you're looking for mileage or framework use-cases you're likely
better off with PocketBase.
Otherwise it may be worth giving TrailBase a closer look, especially when
flexibility and performance matter.

<GettingTrailBase />

---

## TrailBase vs. SupaBase

Both SupaBase and Postgres are amazing. Comparing either to TrailBase and
SQLite, respectively, is challenging given how different they are
architecturally.

For one, both Postgres and SupaBase are heck of a lot more modular. "Rule 34" of
the database world: if you can think of it, there's a Postgres extension for it.
And SupaBase does an excellent job at making all that flexibility available
without getting in the way and giving you untethered access while further
expanding upon it.
In many ways, TrailBase is trying to eventually do the same for SQLite:
combining PocketBase's simplicity with SupaBase's layering.

One foundational difference is that Postgres itself is a multi-user,
client-server architecture already.
Extending it by building a layered services around it, like SupaBase did,
feels very natural.
However, SQLite is neither a multi-user system nor a server. Hence, extending
it by embedding it into a monolith, like PocketBase did,  feels fairly natural
as well.
There are ups and downs to either approach. The layered service approach, for
example, allows for isolated failure domains and scaling of individual
components [^5]. The monolith, on the other hand, with its lesser need for modularity
can have fewer interaction points, fewer moving parts making it fundamentally
simpler, cheaper, and
[lower overhead (10+x performance difference)](/reference/benchmarks).

Ultimately, the biggest difference is that SupaBase is a polished product with
a lot of mileage under its belt. Our simpler architecture will hopefully let us
get there but for now SupaBase is our north star.

---

[^1]:
    We believe that SQL a ubiquitous evergreen technology, which in of itself
    is already a high-level abstraction for efficient, unified cross-database
    access.
    ORMs, on the other hand, often look great in examples but many fall apart
    for more complex tasks. They're certainly bespoke, non-transferable
    knowledge, and lead to vendor lock-in.

[^2]:
    Maybe more in line with SupaBase's philosophy. We suspect that PocketBase
    relies on schema metadata by construction requiring alterations to be
    mediated through PocketBase to keep everything in sync.

[^3]:
   5 All extensions can be built into a small, standalone shared library and
    imported by vanilla SQLite to avoid vendor lock-in.

[^4]:
    Note that SQLite is not strictly typed by default. Instead column types
    merely a type affinity for value conversions.

[^5]:
    For example, in our performance testing we've found that PostgREST,
    SupaBase's RESTful API layer in front of Postgres, is relatively resource
    hungry. This might not be an issue since one can simply scale by pointing
    many independent instances at the same database instance.
