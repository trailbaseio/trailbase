# TrailBase MCP

TrailBase includes an optional native MCP server in the main `trail` binary. It
runs in the same process and container as TrailBase, on the same HTTP port:

```text
TrailBase admin UI: https://trailbase.example.com/_/admin/
TrailBase MCP:      https://trailbase.example.com/mcp
```

There is no MCP sidecar, second Docker image, second port, shared depot mount,
or manually copied bearer token. MCP clients use OAuth to open TrailBase's own
login UI. After an administrator signs in, TrailBase issues and refreshes the
tokens used by the client.

## Enable MCP

MCP is disabled by default because it exposes privileged development and
administration tools. Enable it with `--mcp`:

```sh
trail --public-url https://trailbase.example.com run \
  --address 0.0.0.0:4000 \
  --mcp
```

`--public-url` must be the external HTTPS origin clients use. TrailBase uses it
in OAuth discovery metadata and validates the MCP resource audience against it.
Use HTTPS outside localhost.

The auth UI must also be installed so the browser login page is available:

```sh
trail components add trailbase/auth_ui
```

The official Docker image already contains the auth UI component.

## IDE configuration

Clients with native remote-MCP and OAuth support can connect directly to:

```text
https://trailbase.example.com/mcp
```

For IDEs that accept only local command-based MCP servers, use `mcp-remote`:

```json
{
  "mcpServers": {
    "trailbase": {
      "command": "npx",
      "args": [
        "-y",
        "mcp-remote",
        "https://trailbase.example.com/mcp",
        "--static-oauth-client-metadata",
        "{\"scope\":\"mcp\"}"
      ]
    }
  }
}
```

On the first connection, the client opens a browser at TrailBase's login page.
Sign in with a TrailBase administrator account. Credentials are submitted only
to that TrailBase instance; they are not stored in the IDE configuration or
sent through MCP tool arguments. Existing TrailBase MFA and external identity
provider flows continue to apply.

If the client caches an old failed registration or token, clear its MCP OAuth
cache and reconnect.

## Docker and Portainer

The native server uses the normal `trailbase/trailbase` image. A single-service
Portainer stack is sufficient:

```yaml
services:
  trail:
    image: docker.io/trailbase/trailbase:latest
    ports:
      - "4000:4000"
    restart: unless-stopped
    volumes:
      - /mnt/traildepot:/app/traildepot
    environment:
      RUST_BACKTRACE: "1"
    command:
      - /app/trail
      - --data-dir
      - /app/traildepot
      - --public-url
      - https://trailbase.example.com
      - run
      - --address
      - 0.0.0.0:4000
      - --mcp
```

Point the existing Cloudflare Tunnel or reverse proxy at port `4000`. The same
hostname serves both TrailBase and `/mcp`; no public port `4001` or `8000` is
needed. Do not place Cloudflare Access or another interactive login layer in
front of only `/mcp`, because MCP clients need to reach TrailBase's OAuth
discovery and authorization endpoints. TLS termination at Cloudflare or the
reverse proxy is expected.

## Authentication and security

The native MCP implementation follows the HTTP MCP authorization flow:

- OAuth Protected Resource Metadata (RFC 9728).
- OAuth Authorization Server Metadata (RFC 8414).
- Dynamic Client Registration (RFC 7591).
- Authorization Code flow with PKCE S256.
- MCP access tokens scoped to `mcp` and audience-bound to the instance's
  public `/mcp` resource URL.
- Access-token refresh.
- `WWW-Authenticate` discovery on unauthenticated MCP requests.

Only a currently valid TrailBase administrator can use MCP. TrailBase verifies
administrator status against the database for every MCP HTTP request rather
than trusting the potentially stale `admin` claim in a token. Registering an
OAuth client does not grant access.

Treat MCP as an administrative surface:

- Enable it only when needed.
- Require HTTPS on remote deployments.
- Keep the TrailBase admin login protected with a strong password and MFA.
- Restrict the hostname at the firewall, VPN, Cloudflare policy, or reverse
  proxy when broad internet access is unnecessary.
- Review tool calls before approving destructive schema or data changes.

## Tools

The native server provides focused tools for common work:

- `list_tables`: table, view, column, index, trigger, and metadata discovery.
- `execute_sql`: read or mutate the main database and refresh metadata after
  recognized schema changes.
- `get_config`: return redacted TrailBase protobuf-text configuration.
- `update_config`: validate and save configuration while preserving existing
  secrets.
- `call_admin_api`: reach all remaining admin dashboard operations.

`call_admin_api(method, path, body?)` dispatches directly to TrailBase's
in-process admin router. `path` is relative to `/api/_admin`; it may also be the
full `/api/_admin/...` path. This means MCP and the dashboard use the same Rust
handlers and cannot drift into separate API implementations.

Examples:

```json
{
  "method": "GET",
  "path": "tables"
}
```

```json
{
  "method": "POST",
  "path": "query",
  "body": {
    "query": "CREATE TABLE candy (id INTEGER PRIMARY KEY, name TEXT NOT NULL)"
  }
}
```

Available paths are the same ones used by the admin dashboard, including table,
index, row, file, configuration, JSON Schema, SQL query, user, log, job, backup,
OAuth-provider, and WASM-component operations. TrailBase's normal demo-mode and
handler-level safety checks still apply.

Schema-changing dashboard handlers write migrations and rebuild metadata in the
same way when called through MCP. Raw SQL through `query` also rebuilds schema
metadata for recognized table/view changes.

## Direct bearer-token clients

OAuth is recommended. A client that can explicitly set HTTP headers may instead
send an existing TrailBase administrator access token:

```text
Authorization: Bearer <auth_token>
```

The access token returned by `/api/auth/v1/login` is short-lived. Clients using
this mode must manage `/api/auth/v1/refresh` themselves. Do not put an admin
password or long-lived refresh token in a shared project configuration. This
compatibility mode accepts only ordinary TrailBase tokens without an OAuth
audience; an MCP token minted for another TrailBase resource is rejected.

## Development validation

Run the native MCP unit tests with:

```sh
cargo test -p trailbase --lib mcp::tests
```

For an isolated manual test:

```sh
trail --depot "$(mktemp -d)" \
  --public-url http://127.0.0.1:4100 \
  run --address 127.0.0.1:4100 --dev --mcp
```

Use only a disposable depot for destructive integration tests.
