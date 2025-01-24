FROM node:20-slim AS builder

RUN npm install -g pnpm
RUN pnpm --version

COPY . /app
WORKDIR /app

RUN pnpm install --no-frozen-lockfile
RUN pnpm build


FROM trailbase/trailbase:0.2.2 AS base
USER root
RUN apk add --no-cache sqlite

WORKDIR /app

COPY --from=builder /app/dist /app/public
COPY --chown=trailbase traildepot /app/traildepot

USER trailbase

# Rebuild the database.
COPY arabica_data_cleaned.csv /app/
COPY import.sql /app/
RUN mkdir -p /app/traildepot/data
RUN rm -rf /app/traildepot/data/*
RUN cat import.sql | sqlite3 traildepot/data/main.db

EXPOSE 4000
ENTRYPOINT ["tini", "--"]
CMD ["/app/trail", "--data-dir", "/app/traildepot", "run", "--address", "0.0.0.0:4000", "--public-dir", "/app/public"]

HEALTHCHECK CMD curl --fail http://localhost:4000/api/healthcheck || exit 1
