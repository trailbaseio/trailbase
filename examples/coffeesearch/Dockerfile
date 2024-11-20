FROM trailbase/trailbase:0.2.2 AS base

USER root

RUN apk add --no-cache sqlite

# TODO: Add a dedicated build step, both for (re)building the database and
# building the web app.

COPY dist /app/public
COPY traildepot /app/traildepot
COPY arabica_data_cleaned.csv /app/
COPY import.sql /app/

# Rebuild the database.
WORKDIR /app
RUN mkdir -p /app/traildepot/data
RUN rm -rf /app/traildepot/data/*
RUN cat import.sql | sqlite3 traildepot/data/main.db

RUN chown -R trailbase /app/traildepot/data
USER trailbase

EXPOSE 4000
ENTRYPOINT ["tini", "--"]
CMD ["/app/trail", "--data-dir", "/app/traildepot", "run", "--address", "0.0.0.0:4000", "--public-dir", "/app/public"]

HEALTHCHECK CMD curl --fail http://localhost:4000/api/healthcheck || exit 1
