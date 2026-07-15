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

## Record API file and schema tools

The sidecar exposes TrailBase Record API schemas and file helpers in addition
to normal CRUD:

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
