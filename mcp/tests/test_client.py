from __future__ import annotations

import httpx
import pytest

from trailbase_mcp.client import (
    TrailBaseClient,
    csrf_token_from_jwt,
    file_upload_input,
    is_readonly_sql,
    validate_relative_path,
)
from trailbase_mcp.proto import config_api_pb2
from trailbase_mcp.server import trailbase_request


def test_readonly_sql_detection() -> None:
    assert is_readonly_sql("select * from users")
    assert is_readonly_sql("-- comment\nWITH x AS (select 1) select * from x")
    assert is_readonly_sql("/* comment */ pragma table_info(users)")
    assert not is_readonly_sql("insert into users values (1)")
    assert not is_readonly_sql("select 1; delete from users")


def test_client_sends_bearer_token_and_quotes_path_segments() -> None:
    def handler(request: httpx.Request) -> httpx.Response:
        assert request.headers["authorization"] == "Bearer test-token"
        assert request.url.raw_path == b"/api/records/v1/chat%20messages/id%2F1"
        return httpx.Response(200, json={"id": "id/1"})

    client = TrailBaseClient(
        base_url="http://trailbase.test",
        auth_token="test-token",
        transport=httpx.MockTransport(handler),
    )

    assert client.get_record("chat messages", "id/1") == {"id": "id/1"}


def test_client_passes_record_list_query_parameters() -> None:
    def handler(request: httpx.Request) -> httpx.Response:
        assert request.url.path == "/api/records/v1/venue"
        assert request.url.params["geojson"] == "geometry"
        assert request.url.params["limit"] == "1024"
        assert request.url.params["skip_cursor"] == "true"
        assert request.url.params["cursor"] == "next-page"
        return httpx.Response(200, json={"type": "FeatureCollection", "features": []})

    client = TrailBaseClient(
        base_url="http://trailbase.test",
        transport=httpx.MockTransport(handler),
    )

    assert client.list_records(
        "venue",
        {
            "geojson": "geometry",
            "limit": 1024,
            "skip_cursor": "true",
            "cursor": "next-page",
        },
    ) == {"type": "FeatureCollection", "features": []}


def test_client_generic_trailbase_request() -> None:
    def handler(request: httpx.Request) -> httpx.Response:
        assert request.method == "POST"
        assert request.url.path == "/api/custom/search"
        assert request.url.params["q"] == "coffee"
        assert request.read() == b'{"limit":10}'
        return httpx.Response(200, json={"ok": True})

    client = TrailBaseClient(
        base_url="http://trailbase.test",
        transport=httpx.MockTransport(handler),
    )

    assert client.trailbase_request(
        "POST",
        "/api/custom/search",
        params={"q": "coffee"},
        body={"limit": 10},
    ) == {"ok": True}


def test_generic_trailbase_request_rejects_absolute_urls() -> None:
    assert validate_relative_path("/api/auth/v1/status") == "/api/auth/v1/status"
    with pytest.raises(ValueError, match="server-relative"):
        validate_relative_path("api/auth/v1/status")
    with pytest.raises(ValueError, match="absolute URL"):
        validate_relative_path("https://example.com/api")


def test_server_generic_trailbase_request_write_gate(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.delenv("TRAILBASE_MCP_ENABLE_WRITES", raising=False)

    with pytest.raises(RuntimeError, match="Write operations are disabled"):
        trailbase_request("POST", "/api/auth/v1/login", body={})


def test_client_derives_csrf_header_from_jwt() -> None:
    token = "header.eyJjc3JmX3Rva2VuIjoiY3NyZi0xMjMifQ.signature"

    def handler(request: httpx.Request) -> httpx.Response:
        assert request.headers["csrf-token"] == "csrf-123"
        return httpx.Response(200, json={"ok": True})

    assert csrf_token_from_jwt(token) == "csrf-123"
    client = TrailBaseClient(
        base_url="http://trailbase.test",
        auth_token=token,
        transport=httpx.MockTransport(handler),
    )

    assert client.admin_info() == {"ok": True}


def test_client_raises_with_response_body() -> None:
    client = TrailBaseClient(
        base_url="http://trailbase.test",
        transport=httpx.MockTransport(lambda _request: httpx.Response(401, text="nope")),
    )

    with pytest.raises(RuntimeError, match="HTTP 401: nope"):
        client.admin_info()


def test_client_schema_modes_and_file_download() -> None:
    def handler(request: httpx.Request) -> httpx.Response:
        if request.url.path == "/api/records/v1/widgets/schema":
            assert request.url.params["mode"] == "Select"
            return httpx.Response(200, json={"title": "widgets"})

        if request.url.path == "/api/records/v1/widgets/123/file/avatar":
            return httpx.Response(
                200,
                content=b"hello-file",
                headers={"content-type": "text/plain"},
            )

        raise AssertionError(request.url)

    client = TrailBaseClient(
        base_url="http://trailbase.test",
        transport=httpx.MockTransport(handler),
    )

    assert client.api_json_schema("widgets", mode="Select") == {"title": "widgets"}
    downloaded = client.download_file("widgets", "123", "avatar")
    assert downloaded["content_type"] == "text/plain"
    assert downloaded["content_base64"] == "aGVsbG8tZmlsZQ=="


def test_file_upload_input_and_json_file_create() -> None:
    seen_payload = None

    def handler(request: httpx.Request) -> httpx.Response:
        nonlocal seen_payload
        seen_payload = request.read()
        return httpx.Response(200, json={"ids": ["1"]})

    upload = file_upload_input(
        {
            "field": "avatar",
            "filename": "avatar.txt",
            "content_type": "text/plain",
            "content_base64": "aGVsbG8=",
        }
    )
    assert upload == {
        "name": "avatar",
        "filename": "avatar.txt",
        "content_type": "text/plain",
        "data": "aGVsbG8=",
    }

    client = TrailBaseClient(
        base_url="http://trailbase.test",
        transport=httpx.MockTransport(handler),
    )

    assert client.create_record_with_file_uploads(
        "profiles",
        {"name": "Ada"},
        [
            {
                "field": "avatar",
                "filename": "avatar.txt",
                "content_type": "text/plain",
                "content_base64": "aGVsbG8=",
            },
            {
                "field": "attachments",
                "filename": "notes.txt",
                "content_base64": "bm90ZXM=",
                "multiple": True,
            },
        ],
    ) == {"ids": ["1"]}
    assert seen_payload is not None
    payload = seen_payload.decode()
    assert '"name":"Ada"' in payload
    assert '"avatar":{"data":"aGVsbG8="' in payload
    assert '"attachments":[{"data":"bm90ZXM="' in payload


def test_client_decodes_and_updates_protobuf_config() -> None:
    config_response = config_api_pb2.GetConfigResponse()
    config_response.hash = "hash-1"
    config_response.config.email.smtp_host = "localhost"
    config_response.config.server.application_name = "TrailBase"
    config_response.config.auth.password_minimal_length = 8
    config_response.config.jobs.SetInParent()

    seen_update = None

    def handler(request: httpx.Request) -> httpx.Response:
        nonlocal seen_update
        if request.method == "GET":
            return httpx.Response(200, content=config_response.SerializeToString())

        update = config_api_pb2.UpdateConfigRequest()
        update.ParseFromString(request.content)
        seen_update = update
        return httpx.Response(200, content=b"")

    client = TrailBaseClient(
        base_url="http://trailbase.test",
        transport=httpx.MockTransport(handler),
    )

    decoded = client.admin_config()
    assert decoded["hash"] == "hash-1"
    assert decoded["config"]["server"]["application_name"] == "TrailBase"

    decoded["config"].setdefault("record_apis", []).append(
        {
            "name": "widgets",
            "table_name": "widgets",
            "acl_world": [1, 2, 4, 8, 16],
        }
    )
    assert client.update_config(decoded["config"], decoded["hash"]) == {"ok": True}
    assert seen_update is not None
    assert seen_update.hash == "hash-1"
    assert seen_update.config.record_apis[0].name == "widgets"
