FROM trailbase/trailbase:0.2.1 AS base

COPY traildepot /app/traildepot
COPY dist /app/public

USER root

RUN mkdir -p /app/traildepot/data
RUN chown -R trailbase /app/traildepot/data
RUN chmod +w /app/traildepot/data


USER trailbase

EXPOSE 4000
ENTRYPOINT ["tini", "--"]
CMD ["/app/trail", "--data-dir", "/app/traildepot", "run", "--address", "0.0.0.0:4000", "--public-dir", "/app/public"]

HEALTHCHECK CMD curl --fail http://localhost:4000/api/healthcheck || exit 1
