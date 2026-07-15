from __future__ import annotations

import httpx
import pytest

from trailbase_mcp.client import TrailBaseClient, csrf_token_from_jwt, is_readonly_sql
from trailbase_mcp.proto import config_api_pb2


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
