from __future__ import annotations

import os
import re
import base64
import json
from dataclasses import dataclass
from typing import Any
from urllib.parse import quote

import httpx


TRUE_VALUES = {"1", "true", "yes", "on"}
READONLY_SQL_STARTERS = {"select", "with", "pragma", "explain"}


def env_flag(name: str, default: bool = False) -> bool:
    value = os.getenv(name)
    if value is None:
        return default
    return value.strip().lower() in TRUE_VALUES


def quote_segment(value: str) -> str:
    return quote(value, safe="")


def csrf_token_from_jwt(token: str | None) -> str | None:
    if not token:
        return None

    parts = token.split(".")
    if len(parts) != 3:
        return None

    payload = parts[1]
    payload += "=" * (-len(payload) % 4)
    try:
        claims = json.loads(base64.urlsafe_b64decode(payload))
    except (ValueError, TypeError):
        return None

    csrf_token = claims.get("csrf_token")
    return csrf_token if isinstance(csrf_token, str) and csrf_token else None


def _strip_leading_sql_comments(statement: str) -> str:
    sql = statement.strip()
    while True:
        if sql.startswith("--"):
            _, _, rest = sql.partition("\n")
            sql = rest.strip()
            continue
        if sql.startswith("/*"):
            _, sep, rest = sql.partition("*/")
            if not sep:
                return ""
            sql = rest.strip()
            continue
        return sql


def is_readonly_sql(query: str) -> bool:
    statements = [
        _strip_leading_sql_comments(stmt)
        for stmt in query.split(";")
        if _strip_leading_sql_comments(stmt)
    ]
    if not statements:
        return False

    for statement in statements:
        match = re.match(r"([A-Za-z_]+)", statement)
        if not match or match.group(1).lower() not in READONLY_SQL_STARTERS:
            return False

    return True


@dataclass(slots=True)
class TrailBaseClient:
    base_url: str
    auth_token: str | None = None
    timeout: float = 30.0
    transport: httpx.BaseTransport | None = None

    @classmethod
    def from_env(cls) -> "TrailBaseClient":
        return cls(
            base_url=os.getenv("TRAILBASE_URL", "http://localhost:4000"),
            auth_token=os.getenv("TRAILBASE_AUTH_TOKEN") or os.getenv("TRAILBASE_TOKEN"),
            timeout=float(os.getenv("TRAILBASE_MCP_TIMEOUT", "30")),
        )

    def _headers(self) -> dict[str, str]:
        headers = {
            "accept": "application/json",
            "content-type": "application/json",
        }
        if self.auth_token:
            headers["authorization"] = f"Bearer {self.auth_token}"
            csrf_token = os.getenv("TRAILBASE_CSRF_TOKEN") or csrf_token_from_jwt(self.auth_token)
            if csrf_token:
                headers["csrf-token"] = csrf_token
        return headers

    def request(
        self,
        method: str,
        path: str,
        *,
        params: dict[str, Any] | None = None,
        json: Any | None = None,
    ) -> Any:
        base_url = self.base_url.rstrip("/")
        path = path if path.startswith("/") else f"/{path}"

        with httpx.Client(
            base_url=base_url,
            headers=self._headers(),
            timeout=self.timeout,
            transport=self.transport,
        ) as client:
            response = client.request(method, path, params=params, json=json)

        if response.is_error:
            body = response.text.strip()
            raise RuntimeError(
                f"TrailBase {method.upper()} {path} failed with "
                f"HTTP {response.status_code}: {body}"
            )

        if response.status_code == 204 or not response.content:
            return {"ok": True, "status_code": response.status_code}

        content_type = response.headers.get("content-type", "")
        if "application/json" in content_type:
            return response.json()

        return {
            "ok": True,
            "status_code": response.status_code,
            "body": response.text,
        }

    def admin_info(self) -> Any:
        return self.request("GET", "/api/_admin/info")

    def admin_config(self) -> Any:
        return self.request("GET", "/api/_admin/config")

    def list_tables(self) -> Any:
        return self.request("GET", "/api/_admin/tables")

    def execute_sql(self, query: str, attached_databases: list[str] | None = None) -> Any:
        payload: dict[str, Any] = {"query": query}
        if attached_databases:
            payload["attached_databases"] = attached_databases
        return self.request("POST", "/api/_admin/query", json=payload)

    def api_json_schema(self, api_name: str) -> Any:
        return self.request(
            "GET",
            f"/api/_admin/schema/{quote_segment(api_name)}/schema.json",
        )

    def list_records(
        self,
        api_name: str,
        query: dict[str, Any] | None = None,
    ) -> Any:
        return self.request(
            "GET",
            f"/api/records/v1/{quote_segment(api_name)}",
            params=query,
        )

    def get_record(
        self,
        api_name: str,
        record_id: str,
        expand: str | None = None,
    ) -> Any:
        params = {"expand": expand} if expand else None
        return self.request(
            "GET",
            f"/api/records/v1/{quote_segment(api_name)}/{quote_segment(record_id)}",
            params=params,
        )

    def create_record(self, api_name: str, record: dict[str, Any] | list[dict[str, Any]]) -> Any:
        return self.request(
            "POST",
            f"/api/records/v1/{quote_segment(api_name)}",
            json=record,
        )

    def update_record(self, api_name: str, record_id: str, record: dict[str, Any]) -> Any:
        return self.request(
            "PATCH",
            f"/api/records/v1/{quote_segment(api_name)}/{quote_segment(record_id)}",
            json=record,
        )

    def delete_record(self, api_name: str, record_id: str) -> Any:
        return self.request(
            "DELETE",
            f"/api/records/v1/{quote_segment(api_name)}/{quote_segment(record_id)}",
        )
