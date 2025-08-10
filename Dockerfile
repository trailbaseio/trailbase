FROM lukemathwalker/cargo-chef:latest-rust-1.89-slim AS chef

# Install additional build dependencies. git is needed to bake version metadata.
RUN apt-get update && apt-get install -y --no-install-recommends \
    curl git libssl-dev pkg-config libclang-dev protobuf-compiler libprotobuf-dev libsqlite3-dev

ENV PATH=/usr/local/node/bin:$PATH
ARG NODE_VERSION=22.13.1

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

# First install all JS deps. This is to avoid collisions due to parallel
# installs later-on while building `crates/assets` (auth, admin, client) and
# `crates/js-runtime` (runtime).
RUN pnpm -r install --frozen-lockfile

ARG TARGETPLATFORM

RUN case ${TARGETPLATFORM} in \
         "linux/arm64")  RUST_TARGET="aarch64-unknown-linux-gnu"  ;; \
         *)              RUST_TARGET="x86_64-unknown-linux-gnu"   ;; \
    esac && \
    RUSTFLAGS="-C target-feature=+crt-static" PNPM_OFFLINE="TRUE" cargo build --target ${RUST_TARGET} --release --bin trail && \
    mv target/${RUST_TARGET}/release/trail /app/trail.exe

FROM alpine:3.20 AS runtime
RUN apk add --no-cache tini curl

COPY --from=builder /app/trail.exe /app/trail

# When `docker run` is executed, launch the binary as unprivileged user.
RUN adduser -D trailbase

RUN mkdir -p /app/traildepot
RUN chown trailbase /app/traildepot
USER trailbase

WORKDIR /app

EXPOSE 4000
ENTRYPOINT ["tini", "--"]

CMD ["/app/trail", "--data-dir", "/app/traildepot", "run", "--address", "0.0.0.0:4000"]

HEALTHCHECK CMD curl --fail http://localhost:4000/api/healthcheck || exit 1
