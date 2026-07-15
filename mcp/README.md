# TrailBase MCP sidecar

This package exposes TrailBase through a FastMCP server. It talks to TrailBase
over HTTP and uses the existing admin and record APIs.

Use it as a sidecar container next to TrailBase. The MCP container does not
store TrailBase data; it forwards MCP tool calls to a running TrailBase server.

## Features

- TrailBase admin/runtime info.
- Admin config read/update, including Record API configuration.
- SQL execution with a default read-only guard.
- Table, view, index, and trigger introspection.
- Record API CRUD.
- Record API list query passthrough, including filters, sorting, pagination,
  cursors, `geojson`, `limit`, and `skip_cursor`.
- Record API JSON schemas for `Insert`, `Select`, and `Update`.
- JSON/base64 and multipart file uploads for `std.FileUpload` and
  `std.FileUploads`.
- File download as base64.
- Generic `trailbase_request` tool for custom WASM APIs, auth APIs, and other
  TrailBase HTTP endpoints.

## Quick start with Docker

Run TrailBase separately, then run this MCP sidecar against it.

```sh
docker run --rm -p 8000:8000 \
  -e TRAILBASE_URL=http://host.docker.internal:4000 \
  -e TRAILBASE_AUTH_TOKEN=your-admin-jwt-without-bearer-prefix \
  -e TRAILBASE_MCP_ENABLE_WRITES=false \
  -e MCP_TRANSPORT=http \
  -e MCP_HOST=0.0.0.0 \
  -e MCP_PORT=8000 \
  YOUR_DOCKERHUB_USER/trailbase-mcp:latest
```

The MCP endpoint is:

```text
http://localhost:8000/mcp
```

Do not test `/mcp` in a browser. Use an MCP client. A browser or plain `curl`
request can return `Not Acceptable: Client must accept text/event-stream`,
which is expected for MCP over HTTP.

## Portainer / Docker Compose stack

Replace `YOUR_DOCKERHUB_USER/trailbase-mcp:latest` with your published image.

```yaml
services:
  trail:
    image: docker.io/trailbase/trailbase:latest
    ports:
      - "4000:4000"
    restart: unless-stopped
    volumes:
      - /opt/trailbase/traildepot:/app/traildepot
    environment:
      RUST_BACKTRACE: "1"

  mcp:
    image: YOUR_DOCKERHUB_USER/trailbase-mcp:latest
    depends_on:
      - trail
    ports:
      - "8000:8000"
    restart: unless-stopped
    environment:
      TRAILBASE_URL: "http://trail:4000"
      TRAILBASE_AUTH_TOKEN: "${TRAILBASE_AUTH_TOKEN}"
      TRAILBASE_MCP_ENABLE_WRITES: "false"
      MCP_TRANSPORT: "http"
      MCP_HOST: "0.0.0.0"
      MCP_PORT: "8000"
```

Create the TrailBase data directory before deploying the stack:

```sh
sudo mkdir -p /opt/trailbase/traildepot
sudo chown -R 1000:1000 /opt/trailbase/traildepot
```

If you see TrailBase permission errors, verify the UID used by your TrailBase
image or temporarily relax permissions to confirm the mount is the issue.

## Configuration

Environment variables:

| Variable | Default | Description |
| --- | --- | --- |
| `TRAILBASE_URL` | `http://localhost:4000` | TrailBase base URL. In Compose, use the TrailBase service name, e.g. `http://trail:4000`. |
| `TRAILBASE_AUTH_TOKEN` / `TRAILBASE_TOKEN` | unset | Admin or user JWT used for TrailBase API calls. Pass the raw JWT without the `Bearer ` prefix. |
| `TRAILBASE_CSRF_TOKEN` | derived from JWT | Optional explicit CSRF token. Normally not needed for TrailBase-minted JWTs. |
| `TRAILBASE_MCP_ENABLE_WRITES` | `false` | Set to `true` to enable create/update/delete tools, mutating SQL, config updates, and mutating generic HTTP calls. |
| `TRAILBASE_MCP_TIMEOUT` | `30` | HTTP timeout in seconds. |
| `MCP_TRANSPORT` | `stdio` | Use `http` for container/remote MCP. |
| `MCP_HOST` | `127.0.0.1` | HTTP bind host. Use `0.0.0.0` in Docker. |
| `MCP_PORT` | `8000` | HTTP bind port. |

Mint an admin bearer token with TrailBase:

```sh
cargo run --bin trail -- --data-dir ./traildepot user mint-token admin@localhost
```

Inside the official TrailBase container this is typically:

```sh
/app/trail --data-dir /app/traildepot user mint-token admin@localhost
```

The command prints a value like:

```text
Bearer eyJhbGciOi...
```

Set `TRAILBASE_AUTH_TOKEN` to only the JWT part:

```text
TRAILBASE_AUTH_TOKEN=eyJhbGciOi...
```

## Run with stdio

```sh
cd mcp
python -m venv .venv
. .venv/bin/activate
pip install -e .
TRAILBASE_URL=http://localhost:4000 \
TRAILBASE_AUTH_TOKEN='your-token-without-the-Bearer-prefix' \
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

For local development from this repository, the root `docker-compose.yml`
includes an opt-in `mcp` profile:

```sh
TRAILBASE_AUTH_TOKEN=your-token docker compose --profile mcp up --build
```

The MCP HTTP endpoint is exposed at `http://localhost:8000/mcp`.

## Browser and endpoint notes

- TrailBase's root path (`/`) may return `404`. Use the admin UI path:
  `http://localhost:4000/_/admin/`.
- The MCP HTTP endpoint is not a browser UI. Opening `/mcp` directly in a
  browser or plain `curl` request can return:
  `Not Acceptable: Client must accept text/event-stream`. Use an MCP client,
  such as FastMCP's `Client("http://localhost:8000/mcp")`, which sends the
  required streaming headers.

## Record API file and schema tools

The sidecar exposes TrailBase Record API schemas and file helpers in addition
to normal CRUD:

- `trailbase_request(method, path, params?, body?)`: call any server-relative
  TrailBase HTTP endpoint. Use this for auth endpoints, custom WASM APIs, and
  OpenAPI endpoints not covered by specialized MCP tools. Non-readonly methods
  require `TRAILBASE_MCP_ENABLE_WRITES=true`.
- `list_records(api_name, query?)`: forwards `query` as Record API URL query
  parameters. For example:
  `{"geojson": "geometry", "limit": 1024, "skip_cursor": "true"}` maps to
  `?geojson=geometry&limit=1024&skip_cursor=true`. Cursor pagination works the
  same way with `{"cursor": "<cursor>"}`.
- `get_api_json_schema(api_name, mode?, admin?)`: read a schema from
  `/api/records/v1/<api>/schema`. `mode` may be `Insert`, `Select`, or
  `Update`. Set `admin=true` to use the admin schema endpoint.
- `create_record_with_file_uploads(api_name, record, files)`: create a record
  using JSON/base64 file upload inputs. Each file needs `field` plus either
  `content_base64` or `file_path`; optional fields are `filename`,
  `content_type`, and `multiple`.
- `create_record_multipart(api_name, fields, files)`: create a record as
  `multipart/form-data` using the same file descriptors.
- `download_file(api_name, record_id, column_name, file_name?)`: download a
  `std.FileUpload` or `std.FileUploads` file and return `content_base64`.

## MCP tools

Current tools:

- `trailbase_info`
- `trailbase_config`
- `update_config`
- `list_record_apis`
- `list_tables`
- `execute_sql`
- `trailbase_request`
- `list_records`
- `get_record`
- `create_record`
- `update_record`
- `delete_record`
- `get_api_json_schema`
- `create_record_with_file_uploads`
- `create_record_multipart`
- `download_file`

## Security notes

Treat this sidecar like an admin surface when configured with an admin token.

- Do not expose `/mcp` directly to the public internet.
- Prefer private Docker networks, VPN, mTLS, or an authenticated reverse proxy.
- Keep `TRAILBASE_MCP_ENABLE_WRITES=false` unless the MCP client explicitly
  needs mutation/config/SQL write access.
- Use a least-privilege TrailBase token when possible. Admin tokens are required
  for admin config and SQL tools.
- `trailbase_request` only accepts server-relative paths and cannot proxy to
  arbitrary external URLs.

## Known limitations

- Realtime subscriptions are not exposed as a long-running MCP stream in this
  release.
- TrailBase migrations remain filesystem/CLI driven. MCP can run SQL, but it is
  not a production migration runner.
- The sidecar does not generate language bindings itself; use
  `get_api_json_schema` and an external generator such as quicktype.

## TrailBase documentation compatibility

The MCP sidecar intentionally delegates to TrailBase's public/admin HTTP APIs
instead of reimplementing TrailBase behavior. Current coverage:

- Models & Relations: use `execute_sql` for STRICT tables, constraints,
  indexes, triggers, views, generated columns, geometry columns, and relations;
  use `update_config` to expose tables/views as Record APIs and configure
  `expand`.
- Migrations: TrailBase migrations are filesystem/CLI driven
  (`traildepot/migrations`, `trail migration`, restart/SIGHUP). MCP can apply
  SQL through `execute_sql`, but it is not a migration runner and should not
  replace append-only production migrations.
- Type-Safety: use `get_api_json_schema` with `mode` `Insert`, `Select`, or
  `Update`; feed those schemas into external generators such as quicktype.
- Production: run MCP as a sidecar container and do not expose it publicly
  unless it is protected like an admin surface. The `/mcp` endpoint requires an
  MCP client that accepts `text/event-stream`.
- Custom APIs: use `trailbase_request` for TrailBase WASM/custom routes.
- Record APIs: CRUD, list filters/sort/pagination/cursor/geojson query params,
  schema, JSON/base64 file upload, multipart upload, and file download are
  supported.
- Auth: use `trailbase_request` for auth endpoints; the sidecar itself should
  be configured with an admin token for admin/config operations.
