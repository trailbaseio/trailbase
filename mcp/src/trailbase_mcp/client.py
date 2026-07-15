from __future__ import annotations

import os
import re
import base64
import binascii
import json
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any
from urllib.parse import quote

import httpx
from google.protobuf.json_format import MessageToDict, ParseDict

from .proto import config_api_pb2


TRUE_VALUES = {"1", "true", "yes", "on"}
READONLY_SQL_STARTERS = {"select", "with", "pragma", "explain"}
READONLY_HTTP_METHODS = {"GET", "HEAD", "OPTIONS"}
SQL_IDENTIFIER_RE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")


def env_flag(name: str, default: bool = False) -> bool:
    value = os.getenv(name)
    if value is None:
        return default
    return value.strip().lower() in TRUE_VALUES


def normalize_auth_token(token: str | None) -> str | None:
    if token is None:
        return None
    token = token.strip()
    if not token:
        return None
    if token.lower().startswith("bearer "):
        return token[7:].strip() or None
    return token


def read_text_file(path: str | None) -> str | None:
    if not path:
        return None
    value = Path(path).read_text().strip()
    return value or None


def read_secret_file(path: str | None) -> str | None:
    return normalize_auth_token(read_text_file(path))


def env_or_file(value_name: str, file_name: str) -> str | None:
    value = os.getenv(value_name)
    if value is not None and value.strip():
        return value.strip()
    return read_text_file(os.getenv(file_name))


def auth_token_from_env() -> str | None:
    return normalize_auth_token(
        os.getenv("TRAILBASE_AUTH_TOKEN") or os.getenv("TRAILBASE_TOKEN")
    ) or read_secret_file(
        os.getenv("TRAILBASE_AUTH_TOKEN_FILE")
        or os.getenv("TRAILBASE_TOKEN_FILE")
    )


def refresh_token_from_env() -> str | None:
    return normalize_auth_token(os.getenv("TRAILBASE_REFRESH_TOKEN")) or read_secret_file(
        os.getenv("TRAILBASE_REFRESH_TOKEN_FILE")
    )


def login_email_from_env() -> str | None:
    return (
        env_or_file("TRAILBASE_LOGIN_EMAIL", "TRAILBASE_LOGIN_EMAIL_FILE")
        or env_or_file("TRAILBASE_ADMIN_EMAIL", "TRAILBASE_ADMIN_EMAIL_FILE")
    )


def login_password_from_env() -> str | None:
    return (
        env_or_file("TRAILBASE_LOGIN_PASSWORD", "TRAILBASE_LOGIN_PASSWORD_FILE")
        or env_or_file("TRAILBASE_ADMIN_PASSWORD", "TRAILBASE_ADMIN_PASSWORD_FILE")
    )


def quote_segment(value: str) -> str:
    return quote(value, safe="")


def quote_sql_identifier(value: str) -> str:
    if not isinstance(value, str) or not SQL_IDENTIFIER_RE.fullmatch(value):
        raise ValueError(
            "SQL identifier must start with a letter or underscore and contain "
            "only letters, digits, and underscores"
        )
    return f'"{value}"'


def validate_relative_path(path: str) -> str:
    if not isinstance(path, str):
        raise ValueError("path must be server-relative and start with '/'")
    if path.startswith("//") or "://" in path:
        raise ValueError("path must not be an absolute URL")
    if not path.startswith("/"):
        raise ValueError("path must be server-relative and start with '/'")
    return path


def base64_file_contents(file: dict[str, Any]) -> str:
    content_base64 = file.get("content_base64") or file.get("data")
    file_path = file.get("file_path") or file.get("path")

    if content_base64 is not None and file_path is not None:
        raise ValueError("Provide either content_base64/data or file_path/path, not both")
    if content_base64 is not None:
        if not isinstance(content_base64, str):
            raise ValueError("content_base64/data must be a string")
        return content_base64
    if file_path is not None:
        path = Path(file_path)
        return base64.urlsafe_b64encode(path.read_bytes()).decode()

    raise ValueError("File upload requires content_base64/data or file_path/path")


def decode_base64_contents(value: str) -> bytes:
    padded = value + "=" * (-len(value) % 4)
    try:
        return base64.urlsafe_b64decode(padded)
    except (ValueError, binascii.Error):
        return base64.b64decode(padded)


def file_upload_input(file: dict[str, Any]) -> dict[str, Any]:
    field = file.get("field") or file.get("field_name") or file.get("name")
    if not isinstance(field, str) or not field:
        raise ValueError("File upload requires field/field_name/name")

    filename = file.get("filename")
    if filename is None and (file.get("file_path") or file.get("path")):
        filename = Path(file.get("file_path") or file.get("path")).name

    upload: dict[str, Any] = {
        "name": field,
        "data": base64_file_contents(file),
    }
    if filename is not None:
        upload["filename"] = filename
    if file.get("content_type") is not None:
        upload["content_type"] = file["content_type"]
    return upload


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


def jwt_expires_within(token: str | None, seconds: int) -> bool:
    if not token:
        return True

    parts = token.split(".")
    if len(parts) != 3:
        return False

    payload = parts[1]
    payload += "=" * (-len(payload) % 4)
    try:
        claims = json.loads(base64.urlsafe_b64decode(payload))
    except (ValueError, TypeError):
        return False

    exp = claims.get("exp")
    return isinstance(exp, (int, float)) and exp <= time.time() + seconds


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
    refresh_token: str | None = None
    csrf_token: str | None = None
    login_email: str | None = None
    login_password: str | None = None
    timeout: float = 30.0
    transport: httpx.BaseTransport | None = None

    @classmethod
    def from_env(cls) -> "TrailBaseClient":
        return cls(
            base_url=os.getenv("TRAILBASE_URL", "http://localhost:4000"),
            auth_token=auth_token_from_env(),
            refresh_token=refresh_token_from_env(),
            login_email=login_email_from_env(),
            login_password=login_password_from_env(),
            timeout=float(os.getenv("TRAILBASE_MCP_TIMEOUT", "30")),
        )

    def login(self) -> bool:
        if not self.login_email or not self.login_password:
            return False

        base_url = self.base_url.rstrip("/")
        with httpx.Client(
            base_url=base_url,
            headers={
                "accept": "application/json",
                "content-type": "application/json",
            },
            timeout=self.timeout,
            transport=self.transport,
        ) as client:
            response = client.post(
                "/api/auth/v1/login",
                json={
                    "email": self.login_email,
                    "password": self.login_password,
                    "response_type": "token",
                },
            )

        if response.is_error:
            return False

        body = response.json()
        auth_token = normalize_auth_token(body.get("auth_token"))
        if not auth_token:
            return False

        self.auth_token = auth_token
        self.refresh_token = normalize_auth_token(body.get("refresh_token")) or self.refresh_token
        self.csrf_token = body.get("csrf_token")
        return True

    def refresh_auth_token(self) -> bool:
        if not self.refresh_token:
            return False

        base_url = self.base_url.rstrip("/")
        with httpx.Client(
            base_url=base_url,
            headers={
                "accept": "application/json",
                "content-type": "application/json",
            },
            timeout=self.timeout,
            transport=self.transport,
        ) as client:
            response = client.post(
                "/api/auth/v1/refresh",
                json={"refresh_token": self.refresh_token},
            )

        if response.is_error:
            return False

        body = response.json()
        auth_token = normalize_auth_token(body.get("auth_token"))
        if not auth_token:
            return False

        self.auth_token = auth_token
        self.csrf_token = body.get("csrf_token")
        return True

    def _headers(self) -> dict[str, str]:
        if jwt_expires_within(self.auth_token, 60):
            if not self.refresh_token or not self.refresh_auth_token():
                self.login()

        headers = {
            "accept": "application/json",
            "content-type": "application/json",
        }
        if self.auth_token:
            headers["authorization"] = f"Bearer {self.auth_token}"
            csrf_token = (
                os.getenv("TRAILBASE_CSRF_TOKEN")
                or self.csrf_token
                or csrf_token_from_jwt(self.auth_token)
            )
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
        try:
            return response.json()
        except ValueError:
            pass

        return {
            "ok": True,
            "status_code": response.status_code,
            "body": response.text,
        }

    def request_raw(
        self,
        method: str,
        path: str,
        *,
        params: dict[str, Any] | None = None,
    ) -> httpx.Response:
        base_url = self.base_url.rstrip("/")
        path = path if path.startswith("/") else f"/{path}"
        headers = self._headers()
        headers["accept"] = "*/*"
        headers.pop("content-type", None)

        with httpx.Client(
            base_url=base_url,
            headers=headers,
            timeout=self.timeout,
            transport=self.transport,
        ) as client:
            response = client.request(method, path, params=params)

        if response.is_error:
            body = response.text.strip()
            raise RuntimeError(
                f"TrailBase {method.upper()} {path} failed with "
                f"HTTP {response.status_code}: {body}"
            )

        return response

    def request_multipart(
        self,
        method: str,
        path: str,
        *,
        data: dict[str, str],
        files: list[tuple[str, tuple[str, bytes, str | None]]],
    ) -> Any:
        base_url = self.base_url.rstrip("/")
        path = path if path.startswith("/") else f"/{path}"
        headers = self._headers()
        headers.pop("content-type", None)

        with httpx.Client(
            base_url=base_url,
            headers=headers,
            timeout=self.timeout,
            transport=self.transport,
        ) as client:
            response = client.request(method, path, data=data, files=files)

        if response.is_error:
            body = response.text.strip()
            raise RuntimeError(
                f"TrailBase {method.upper()} {path} failed with "
                f"HTTP {response.status_code}: {body}"
            )

        if response.status_code == 204 or not response.content:
            return {"ok": True, "status_code": response.status_code}
        try:
            return response.json()
        except ValueError:
            return {
                "ok": True,
                "status_code": response.status_code,
                "body": response.text,
            }

    def request_bytes(
        self,
        method: str,
        path: str,
        *,
        body: bytes | None = None,
    ) -> bytes:
        base_url = self.base_url.rstrip("/")
        path = path if path.startswith("/") else f"/{path}"

        headers = self._headers()
        headers["content-type"] = "application/protobuf"
        headers["accept"] = "application/protobuf"

        with httpx.Client(
            base_url=base_url,
            headers=headers,
            timeout=self.timeout,
            transport=self.transport,
        ) as client:
            response = client.request(method, path, content=body)

        if response.is_error:
            body_text = response.text.strip()
            raise RuntimeError(
                f"TrailBase {method.upper()} {path} failed with "
                f"HTTP {response.status_code}: {body_text}"
            )

        return response.content

    def admin_info(self) -> Any:
        return self.request("GET", "/api/_admin/info")

    def admin_config(self) -> Any:
        response = config_api_pb2.GetConfigResponse()
        response.ParseFromString(self.request_bytes("GET", "/api/_admin/config"))
        return MessageToDict(
            response,
            preserving_proto_field_name=True,
            use_integers_for_enums=True,
        )

    def update_config(self, config: dict[str, Any], hash: str) -> Any:
        request = ParseDict(
            {"config": config, "hash": hash},
            config_api_pb2.UpdateConfigRequest(),
        )
        self.request_bytes(
            "POST",
            "/api/_admin/config",
            body=request.SerializeToString(),
        )
        return {"ok": True}

    def remove_record_api(
        self,
        api_name: str | None = None,
        table_name: str | None = None,
    ) -> Any:
        if not api_name and not table_name:
            raise ValueError("Provide api_name or table_name")

        response = self.admin_config()
        config = response["config"]
        record_apis = config.setdefault("record_apis", [])
        kept = []
        removed = []

        for api in record_apis:
            name_matches = api_name is not None and api.get("name") == api_name
            table_matches = table_name is not None and api.get("table_name") == table_name
            if name_matches or table_matches:
                removed.append(api)
            else:
                kept.append(api)

        if not removed:
            return {"ok": True, "removed": [], "updated": False}

        config["record_apis"] = kept
        self.update_config(config, response["hash"])
        return {"ok": True, "removed": removed, "updated": True}

    def drop_table(
        self,
        table_name: str,
        remove_record_apis: bool = True,
    ) -> Any:
        removed_apis = {"ok": True, "removed": [], "updated": False}
        if remove_record_apis:
            removed_apis = self.remove_record_api(table_name=table_name)

        sql = f"DROP TABLE IF EXISTS {quote_sql_identifier(table_name)}"
        dropped = self.execute_sql(sql)
        return {
            "ok": True,
            "table_name": table_name,
            "removed_record_apis": removed_apis.get("removed", []),
            "drop_result": dropped,
        }

    def list_tables(self) -> Any:
        return self.request("GET", "/api/_admin/tables")

    def execute_sql(self, query: str, attached_databases: list[str] | None = None) -> Any:
        payload: dict[str, Any] = {"query": query}
        if attached_databases:
            payload["attached_databases"] = attached_databases
        return self.request("POST", "/api/_admin/query", json=payload)

    def trailbase_request(
        self,
        method: str,
        path: str,
        *,
        params: dict[str, Any] | None = None,
        body: Any | None = None,
    ) -> Any:
        return self.request(
            method.upper(),
            validate_relative_path(path),
            params=params,
            json=body,
        )

    def api_json_schema(
        self,
        api_name: str,
        mode: str | None = None,
        admin: bool = False,
    ) -> Any:
        params = {"mode": mode} if mode else None
        path = (
            f"/api/_admin/schema/{quote_segment(api_name)}/schema.json"
            if admin
            else f"/api/records/v1/{quote_segment(api_name)}/schema"
        )
        return self.request("GET", path, params=params)

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

    def create_record_with_file_uploads(
        self,
        api_name: str,
        record: dict[str, Any],
        files: list[dict[str, Any]],
    ) -> Any:
        payload = dict(record)
        for file in files:
            upload = file_upload_input(file)
            field = upload["name"]
            upload_for_record = dict(upload)
            upload_for_record.pop("name", None)

            existing = payload.get(field)
            if file.get("multiple"):
                if existing is None:
                    payload[field] = [upload_for_record]
                elif isinstance(existing, list):
                    existing.append(upload_for_record)
                else:
                    payload[field] = [existing, upload_for_record]
            elif existing is None:
                payload[field] = upload_for_record
            elif isinstance(existing, list):
                existing.append(upload_for_record)
            else:
                payload[field] = [existing, upload_for_record]

        return self.create_record(api_name, payload)

    def create_record_multipart(
        self,
        api_name: str,
        fields: dict[str, Any],
        files: list[dict[str, Any]],
    ) -> Any:
        data: dict[str, str] = {}
        for key, value in fields.items():
            if isinstance(value, list):
                data[key] = json.dumps(value)
            elif value is not None:
                data[key] = str(value)

        multipart_files: list[tuple[str, tuple[str, bytes, str | None]]] = []
        for file in files:
            field = file.get("field") or file.get("field_name") or file.get("name")
            if not isinstance(field, str) or not field:
                raise ValueError("Multipart file requires field/field_name/name")

            file_path = file.get("file_path") or file.get("path")
            content_base64 = file.get("content_base64") or file.get("data")
            if file_path is not None and content_base64 is not None:
                raise ValueError("Provide either content_base64/data or file_path/path, not both")
            if file_path is not None:
                bytes_data = Path(file_path).read_bytes()
                filename = file.get("filename") or Path(file_path).name
            elif content_base64 is not None:
                bytes_data = decode_base64_contents(content_base64)
                filename = file.get("filename") or field
            else:
                raise ValueError("Multipart file requires content_base64/data or file_path/path")

            multipart_files.append(
                (
                    field,
                    (
                        str(filename),
                        bytes_data,
                        file.get("content_type"),
                    ),
                )
            )

        return self.request_multipart(
            "POST",
            f"/api/records/v1/{quote_segment(api_name)}",
            data=data,
            files=multipart_files,
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

    def download_file(
        self,
        api_name: str,
        record_id: str,
        column_name: str,
        file_name: str | None = None,
    ) -> Any:
        path = (
            f"/api/records/v1/{quote_segment(api_name)}/"
            f"{quote_segment(record_id)}/"
            f"{'files' if file_name else 'file'}/"
            f"{quote_segment(column_name)}"
        )
        if file_name:
            path += f"/{quote_segment(file_name)}"

        response = self.request_raw("GET", path)
        return {
            "ok": True,
            "status_code": response.status_code,
            "content_type": response.headers.get("content-type"),
            "content_disposition": response.headers.get("content-disposition"),
            "content_length": len(response.content),
            "content_base64": base64.b64encode(response.content).decode(),
        }
