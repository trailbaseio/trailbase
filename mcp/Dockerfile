FROM python:3.12-slim

LABEL org.opencontainers.image.title="TrailBase MCP sidecar"
LABEL org.opencontainers.image.description="FastMCP sidecar server for TrailBase"
LABEL org.opencontainers.image.version="0.2.0"
LABEL org.opencontainers.image.source="https://github.com/trailbaseio/trailbase"
LABEL org.opencontainers.image.url="https://hub.docker.com/r/frostbite4456/trailbase-mcp"

ENV PYTHONDONTWRITEBYTECODE=1
ENV PYTHONUNBUFFERED=1

WORKDIR /app
COPY pyproject.toml README.md ./
COPY src ./src

RUN pip install --no-cache-dir .

EXPOSE 8000
CMD ["python", "-m", "trailbase_mcp.server"]
