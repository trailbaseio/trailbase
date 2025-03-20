FROM node:20-slim AS builder

RUN npm install -g pnpm
RUN pnpm --version

COPY web /app
WORKDIR /app

RUN pnpm install --no-frozen-lockfile
RUN pnpm build


FROM trailbase/trailbase:latest AS base

WORKDIR /app

COPY --from=builder /app/dist /app/public
COPY --chown=trailbase traildepot /app/traildepot

USER trailbase

EXPOSE 4000
ENTRYPOINT ["tini", "--"]
CMD ["/app/trail", "--data-dir", "/app/traildepot", "run", "--address", "0.0.0.0:4000", "--public-dir", "/app/public"]

HEALTHCHECK CMD curl --fail http://localhost:4000/api/healthcheck || exit 1
