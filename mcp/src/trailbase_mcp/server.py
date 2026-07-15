from __future__ import annotations

import os
from typing import Any

from fastmcp import FastMCP

from .client import TrailBaseClient, env_flag, is_readonly_sql


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
    """Return TrailBase configuration, including configured record APIs."""
    return _client().admin_config()


@mcp.tool
def list_record_apis() -> Any:
    """List configured TrailBase record APIs from the server config."""
    config = _client().admin_config()
    return config.get("record_apis", [])


@mcp.tool
def list_tables() -> Any:
    """List TrailBase tables, views, indexes, and triggers."""
    return _client().list_tables()


@mcp.tool
def get_api_json_schema(api_name: str) -> Any:
    """Return the JSON Schema for a configured TrailBase record API."""
    return _client().api_json_schema(api_name)


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
def update_record(api_name: str, record_id: str, record: dict[str, Any]) -> Any:
    """Update one record. Requires TRAILBASE_MCP_ENABLE_WRITES=true."""
    _require_writes_enabled()
    return _client().update_record(api_name, record_id, record)


@mcp.tool
def delete_record(api_name: str, record_id: str) -> Any:
    """Delete one record. Requires TRAILBASE_MCP_ENABLE_WRITES=true."""
    _require_writes_enabled()
    return _client().delete_record(api_name, record_id)


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
