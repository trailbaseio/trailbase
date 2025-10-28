# Using the following docker base images, because the `ring` crate is a bit
# iffy to compile. Tore my hair out with debian:
#    https://github.com/briansmith/ring/issues/1414
FROM messense/rust-musl-cross:x86_64-musl AS builder-amd64
FROM messense/rust-musl-cross:aarch64-musl AS builder-arm64

ARG TARGETARCH

FROM builder-${TARGETARCH} AS setup-builder

# Install additional build dependencies. git is needed to bake version metadata.
RUN apt-get update && apt-get install -y --no-install-recommends \
    curl git libssl-dev pkg-config libclang-dev protobuf-compiler libprotobuf-dev libsqlite3-dev

# Install node
ENV PATH=/usr/local/node/bin:$PATH
ARG NODE_VERSION=22.13.1

RUN curl -sL https://github.com/nodenv/node-build/archive/master.tar.gz | tar xz -C /tmp/ && \
    /tmp/node-build-master/bin/node-build "${NODE_VERSION}" /usr/local/node && \
    rm -rf /tmp/node-build-master

RUN npm install -g pnpm
RUN pnpm --version


FROM setup-builder AS builder

WORKDIR /app
COPY . .

# First install all JS dependencies. This is to avoid `node_modules` collisions
# due to parallel installs later-on while building packages for various crates.
RUN pnpm -r install --frozen-lockfile

ARG TARGETPLATFORM

RUN case ${TARGETPLATFORM} in \
         "linux/arm64")  RUST_TARGET="aarch64-unknown-linux-musl"  ;; \
         *)              RUST_TARGET="x86_64-unknown-linux-musl"   ;; \
    esac && \
    RUST_BACKTRACE=1 PNPM_OFFLINE="TRUE" cargo build --target ${RUST_TARGET} --features=vendor-ssl --release --bin trail && \
    mv target/${RUST_TARGET}/release/trail /app/trail.exe


FROM alpine:3.22 AS runtime
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
