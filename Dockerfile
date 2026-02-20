# NOTE: We cannot use alpine here because rusqlite's `libsqlite3-sys` with
# `preupdate-hook` has a **build-time** dependency on the `bindgen` crate with
# the `runtime` feature enabled. This in turn requires a dynamically linked
# libclang.so, alpine's `clang-dev` package won't work :/.
FROM messense/rust-musl-cross:x86_64-musl AS builder-amd64
FROM messense/rust-musl-cross:aarch64-musl AS builder-arm64

ARG TARGETARCH

FROM builder-${TARGETARCH} AS base-builder

# Install additional build dependencies. git is needed to bake version metadata.
RUN apt-get update && apt-get install -y --no-install-recommends \
    curl git libssl-dev pkg-config libclang-dev protobuf-compiler libprotobuf-dev libsqlite3-dev

# Install node
ENV PATH=/usr/local/node/bin:$PATH
ENV NODE_VERSION=22.13.1

RUN curl -sL https://github.com/nodenv/node-build/archive/master.tar.gz | tar xz -C /tmp/ && \
    /tmp/node-build-master/bin/node-build "${NODE_VERSION}" /usr/local/node && \
    rm -rf /tmp/node-build-master

RUN npm install -g pnpm
RUN pnpm --version

WORKDIR /app
COPY . .

# Start by installing all JS dependencies upfront. This is to avoid
# `node_modules` collisions due to parallel installs later-on while building
# packages for various crates.
RUN pnpm -r install --frozen-lockfile


FROM base-builder AS auth-ui-builder

RUN rustup target add wasm32-wasip2
RUN RUST_BACKTRACE=1 PNPM_OFFLINE="TRUE" cargo build --target wasm32-wasip2 --release -p auth-ui-component


FROM base-builder AS binary-builder

ARG TARGETPLATFORM

RUN case ${TARGETPLATFORM} in \
         "linux/arm64")  RUST_TARGET="aarch64-unknown-linux-musl"  ;; \
         *)              RUST_TARGET="x86_64-unknown-linux-musl"   ;; \
    esac && \
    RUST_BACKTRACE=1 PNPM_OFFLINE="TRUE" cargo build --target ${RUST_TARGET} --features=geos-static --release --bin trail && \
    mv target/${RUST_TARGET}/release/trail /app/trail.exe


FROM alpine:3.22 AS image
RUN apk add --no-cache tini curl

RUN mkdir -p /app/traildepot/wasm

COPY --from=binary-builder /app/trail.exe /app/trail
COPY --from=auth-ui-builder /app/target/wasm32-wasip2/release/auth_ui_component.wasm /app/traildepot/wasm/

# When `docker run` is executed, launch the binary as unprivileged user.
RUN adduser -D trailbase
RUN chown trailbase /app/traildepot
USER trailbase

WORKDIR /app

EXPOSE 4000
ENTRYPOINT ["tini", "--"]

CMD ["/app/trail", "--data-dir", "/app/traildepot", "run", "--address", "0.0.0.0:4000"]

HEALTHCHECK CMD curl --fail http://localhost:4000/api/healthcheck || exit 1
