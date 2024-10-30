# syntax = edrevo/dockerfile-plus

# NOTE: paths are relative to build context, which is trailbase's root otherwise we
# cannot build the trailbase server as well.

INCLUDE+ Dockerfile

FROM chef AS webapp_builder

COPY examples/blog/web /app
WORKDIR /app

RUN pnpm install --no-frozen-lockfile
RUN pnpm run build

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends tini curl

COPY --from=builder /app/target/x86_64-unknown-linux-gnu/release/trail /app/
COPY --from=webapp_builder /app/dist /app/public

# When `docker run` is executed, launch the binary as unprivileged user.
ENV USERNAME=trailbase
RUN adduser \
    --disabled-password \
    --gecos "" \
    --home "$(pwd)" \
    --no-create-home \
    ${USERNAME}
USER ${USERNAME}

WORKDIR /app

EXPOSE 4000
ENTRYPOINT ["tini", "--"]

CMD ["/app/trail", "run"]

HEALTHCHECK CMD curl --fail http://localhost:4000/api/healthcheck || exit 1
