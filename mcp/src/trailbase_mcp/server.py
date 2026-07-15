from __future__ import annotations

import os
from typing import Any

from fastmcp import FastMCP

from .client import READONLY_HTTP_METHODS, TrailBaseClient, env_flag, is_readonly_sql
from .endpoints import get_api_operation, list_api_operations, render_operation_path


mcp = FastMCP("TrailBase")


def _client() -> TrailBaseClient:
    return TrailBaseClient.from_env()


def _require_writes_enabled() -> None:
    if not env_flag("TRAILBASE_MCP_ENABLE_WRITES"):
        raise RuntimeError(
            "Write operations are disabled. Set TRAILBASE_MCP_ENABLE_WRITES=true "
            "for this MCP server process to enable mutating tools."
        )


@mcp.tool
def trailbase_info() -> Any:
    """Return TrailBase server build/runtime metadata."""
    return _client().admin_info()


@mcp.tool
def trailbase_config() -> Any:
    """Return TrailBase configuration plus the config hash required for updates."""
    return _client().admin_config()


@mcp.tool
def update_config(config: dict[str, Any], hash: str) -> Any:
    """Replace TrailBase configuration. Requires TRAILBASE_MCP_ENABLE_WRITES=true."""
    _require_writes_enabled()
    return _client().update_config(config, hash)


@mcp.tool
def list_record_apis() -> Any:
    """List configured TrailBase record APIs from the server config."""
    response = _client().admin_config()
    return {"record_apis": response.get("config", {}).get("record_apis", [])}


@mcp.tool
def list_tables() -> Any:
    """List TrailBase tables, views, indexes, and triggers."""
    return _client().list_tables()


@mcp.tool
def get_api_json_schema(
    api_name: str,
    mode: str | None = None,
    admin: bool = False,
) -> Any:
    """Return the JSON Schema for a configured TrailBase record API.

    mode may be Insert, Select, or Update. By default this uses the public
    record schema endpoint; set admin=True to use the admin schema endpoint.
    """
    return _client().api_json_schema(api_name, mode, admin)


@mcp.tool
def execute_sql(
    query: str,
    attached_databases: list[str] | None = None,
    allow_mutation: bool = False,
) -> Any:
    """Execute SQL through TrailBase's admin query endpoint.

    By default this accepts only read-oriented statements. Mutations require both
    allow_mutation=True and TRAILBASE_MCP_ENABLE_WRITES=true.
    """
    if not allow_mutation and not is_readonly_sql(query):
        raise RuntimeError(
            "Only SELECT/WITH/PRAGMA/EXPLAIN statements are allowed by default. "
            "Set allow_mutation=True and TRAILBASE_MCP_ENABLE_WRITES=true to run mutations."
        )
    if allow_mutation:
        _require_writes_enabled()

    return _client().execute_sql(query, attached_databases)


@mcp.tool
def trailbase_request(
    method: str,
    path: str,
    params: dict[str, Any] | None = None,
    body: Any | None = None,
) -> Any:
    """Call an arbitrary TrailBase HTTP endpoint on the configured server.

    Use this for custom WASM APIs, auth endpoints, OpenAPI endpoints, and other
    TrailBase routes not covered by a specialized MCP tool. The path must be
    server-relative, e.g. /api/auth/v1/status. Non-readonly methods require
    TRAILBASE_MCP_ENABLE_WRITES=true.
    """
    normalized_method = method.upper()
    if normalized_method not in READONLY_HTTP_METHODS:
        _require_writes_enabled()
    return _client().trailbase_request(normalized_method, path, params=params, body=body)


@mcp.tool
def list_trailbase_api_operations(category: str | None = None) -> Any:
    """List TrailBase OpenAPI operations known to this MCP server.

    category may be auth, oauth, or records. The response includes the
    operation_id, HTTP method, server-relative path template, mutation gate, and
    recommended MCP support path for each operation.
    """
    return {"operations": list_api_operations(category)}


@mcp.tool
def call_trailbase_api_operation(
    operation_id: str,
    path_params: dict[str, Any] | None = None,
    params: dict[str, Any] | None = None,
    body: Any | None = None,
) -> Any:
    """Call a known TrailBase OpenAPI operation by operation_id.

    path_params fills path templates such as {"name": "todos", "record": "id"}.
    params are URL query parameters and body is sent as JSON. Mutating
    operations require TRAILBASE_MCP_ENABLE_WRITES=true. Streaming SSE
    operations are cataloged but not proxied by this request/response tool.
    """
    operation = get_api_operation(operation_id)
    if operation.get("streaming"):
        raise RuntimeError(
            f"{operation_id} is a long-running streaming endpoint and is not "
            "proxied by this request/response MCP tool."
        )
    if operation.get("requires_write_permission"):
        _require_writes_enabled()

    path = render_operation_path(operation, path_params)
    return _client().trailbase_request(operation["method"], path, params=params, body=body)


@mcp.tool
def list_records(api_name: str, query: dict[str, Any] | None = None) -> Any:
    """List records for a TrailBase record API.

    The optional query object is passed as URL query parameters, e.g.
    {"limit": 20, "count": true}.
    """
    return _client().list_records(api_name, query)


@mcp.tool
def get_record(api_name: str, record_id: str, expand: str | None = None) -> Any:
    """Fetch one record from a TrailBase record API."""
    return _client().get_record(api_name, record_id, expand)


@mcp.tool
def create_record(api_name: str, record: dict[str, Any] | list[dict[str, Any]]) -> Any:
    """Create one or more records. Requires TRAILBASE_MCP_ENABLE_WRITES=true."""
    _require_writes_enabled()
    return _client().create_record(api_name, record)


@mcp.tool
def create_record_with_file_uploads(
    api_name: str,
    record: dict[str, Any],
    files: list[dict[str, Any]],
) -> Any:
    """Create one record with JSON/base64 TrailBase file upload inputs.

    Each file requires field/field_name/name plus either content_base64/data or
    file_path/path. Optional keys: filename, content_type, multiple.
    """
    _require_writes_enabled()
    return _client().create_record_with_file_uploads(api_name, record, files)


@mcp.tool
def create_record_multipart(
    api_name: str,
    fields: dict[str, Any],
    files: list[dict[str, Any]],
) -> Any:
    """Create one record as multipart/form-data with file parts.

    Each file requires field/field_name/name plus either content_base64/data or
    file_path/path. Optional keys: filename, content_type.
    """
    _require_writes_enabled()
    return _client().create_record_multipart(api_name, fields, files)


@mcp.tool
def update_record(api_name: str, record_id: str, record: dict[str, Any]) -> Any:
    """Update one record. Requires TRAILBASE_MCP_ENABLE_WRITES=true."""
    _require_writes_enabled()
    return _client().update_record(api_name, record_id, record)


@mcp.tool
def delete_record(api_name: str, record_id: str) -> Any:
    """Delete one record. Requires TRAILBASE_MCP_ENABLE_WRITES=true."""
    _require_writes_enabled()
    return _client().delete_record(api_name, record_id)


@mcp.tool
def download_file(
    api_name: str,
    record_id: str,
    column_name: str,
    file_name: str | None = None,
) -> Any:
    """Download a TrailBase file column and return the bytes as content_base64.

    For std.FileUpload columns omit file_name. For std.FileUploads columns pass
    the metadata filename as file_name.
    """
    return _client().download_file(api_name, record_id, column_name, file_name)


def main() -> None:
    transport = os.getenv("MCP_TRANSPORT", "stdio")
    if transport == "http":
        mcp.run(
            transport="http",
            host=os.getenv("MCP_HOST", "127.0.0.1"),
            port=int(os.getenv("MCP_PORT", "8000")),
        )
    else:
        mcp.run()


if __name__ == "__main__":
    main()
