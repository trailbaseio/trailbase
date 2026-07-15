# TrailBase MCP sidecar

This package exposes TrailBase through a FastMCP server. It talks to TrailBase
over HTTP and uses the existing admin and record APIs.

## Configuration

Environment variables:

- `TRAILBASE_URL`: TrailBase base URL. Defaults to `http://localhost:4000`.
- `TRAILBASE_AUTH_TOKEN` or `TRAILBASE_TOKEN`: bearer token used for admin and
  protected record APIs.
- `TRAILBASE_MCP_ENABLE_WRITES`: set to `true` to enable create/update/delete
  tools and mutating SQL.
- `MCP_TRANSPORT`: `stdio` by default; set to `http` for remote MCP.
- `MCP_HOST`: HTTP bind host, default `127.0.0.1`.
- `MCP_PORT`: HTTP bind port, default `8000`.

Mint an admin bearer token with TrailBase:

```sh
cargo run --bin trail -- --data-dir ./traildepot user mint-token admin@localhost
```

## Run with stdio

```sh
cd mcp
python -m venv .venv
. .venv/bin/activate
pip install -e .
TRAILBASE_URL=http://localhost:4000 \
TRAILBASE_AUTH_TOKEN='Bearer token without the Bearer prefix' \
python -m trailbase_mcp.server
```

Example MCP client config:

```json
{
  "mcpServers": {
    "trailbase": {
      "command": "python",
      "args": ["-m", "trailbase_mcp.server"],
      "env": {
        "TRAILBASE_URL": "http://localhost:4000",
        "TRAILBASE_AUTH_TOKEN": "your-token"
      }
    }
  }
}
```

## Run with Docker Compose

The root `docker-compose.yml` includes an opt-in `mcp` profile:

```sh
TRAILBASE_AUTH_TOKEN=your-token docker compose --profile mcp up --build
```

The MCP HTTP endpoint is exposed at `http://localhost:8000/mcp`.
