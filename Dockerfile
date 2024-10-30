FROM lukemathwalker/cargo-chef:latest-rust-1.81-slim AS chef

# Install additional build dependencies.
#
# NOTE: we should consider building sqlean against
# libsql/libsql-sqlite3/src/sqlite3ext.h rather than upstrean libsqlite3-dev
# for increased consistency.
RUN apt-get update && apt-get install -y --no-install-recommends curl libssl-dev pkg-config libclang-dev protobuf-compiler libprotobuf-dev libsqlite3-dev

ENV PATH=/usr/local/node/bin:$PATH
ARG NODE_VERSION=22.9.0

RUN curl -sL https://github.com/nodenv/node-build/archive/master.tar.gz | tar xz -C /tmp/ && \
    /tmp/node-build-master/bin/node-build "${NODE_VERSION}" /usr/local/node && \
    rm -rf /tmp/node-build-master

RUN npm install -g pnpm
RUN pnpm --version

FROM chef AS planner
WORKDIR /app
COPY . .
RUN cargo chef prepare --recipe-path recipe.json


FROM planner AS builder
# Re-build dependencies in case they have changed.
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

COPY . .

RUN RUSTFLAGS="-C target-feature=+crt-static" cargo build --target x86_64-unknown-linux-gnu --release --bin trail

FROM alpine:3.20 AS runtime
RUN apk add --no-cache tini curl

COPY --from=builder /app/target/x86_64-unknown-linux-gnu/release/trail /app/

# When `docker run` is executed, launch the binary as unprivileged user.
RUN adduser -D trailbase
USER trailbase

WORKDIR /app

EXPOSE 4000
ENTRYPOINT ["tini", "--"]

CMD ["/app/trail", "--data-dir", "/app/traildepot", "run", "--address", "0.0.0.0:4000"]

HEALTHCHECK CMD curl --fail http://localhost:4000/api/healthcheck || exit 1
