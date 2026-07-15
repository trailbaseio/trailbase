# TrailBase MCP sidecar

This package exposes TrailBase through a FastMCP server. It talks to TrailBase
over HTTP and uses the existing admin and record APIs.

## Configuration

Environment variables:

- `TRAILBASE_URL`: TrailBase base URL. Defaults to `http://localhost:4000`.
- `TRAILBASE_AUTH_TOKEN` or `TRAILBASE_TOKEN`: bearer token used for admin and
  protected record APIs. Pass the raw JWT without the `Bearer ` prefix.
- `TRAILBASE_CSRF_TOKEN`: optional explicit CSRF token. If omitted, the sidecar
  derives it from TrailBase JWTs for admin API calls.
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
