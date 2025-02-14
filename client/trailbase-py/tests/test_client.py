from trailbase import Client, RecordId, JSON, JSON_OBJECT

import httpx
import logging
import os
import pytest
import subprocess

from time import time, sleep
from typing import List

logging.basicConfig(level=logging.DEBUG)

port = 4007
address = f"127.0.0.1:{port}"
site = f"http://{address}"


class TrailBaseFixture:
    process: None | subprocess.Popen[bytes]

    def __init__(self) -> None:
        cwd = os.getcwd()
        traildepot = "../testfixture" if cwd.endswith("trailbase-py") else "client/testfixture"

        logger.info("Building TrailBase")
        build = subprocess.run(["cargo", "build"])
        assert build.returncode == 0

        logger.info("Starting TrailBase")
        self.process = subprocess.Popen(
            [
                "cargo",
                "run",
                "--",
                "--data-dir",
                traildepot,
                "run",
                "-a",
                address,
                "--js-runtime-threads",
                "1",
            ]
        )

        client = httpx.Client()
        for _ in range(100):
            try:
                response = client.get(f"http://{address}/api/healthcheck")
                if response.status_code == 200:
                    return
            except:
                pass

            sleep(0.5)

        logger.error("Failed ot start TrailBase")

    def isUp(self) -> bool:
        p = self.process
        return p != None and p.returncode == None

    def shutdown(self) -> None:
        p = self.process
        if p != None:
            p.send_signal(9)
            p.wait()
            assert isinstance(p.returncode, int)


@pytest.fixture(scope="session")
def trailbase():
    fixture = TrailBaseFixture()
    yield fixture
    fixture.shutdown()


def connect() -> Client:
    client = Client(site, tokens=None)
    client.login("admin@localhost", "secret")
    return client


def test_client_login(trailbase: TrailBaseFixture):
    assert trailbase.isUp()

    client = connect()
    assert client.site() == site

    tokens = client.tokens()
    assert tokens != None and tokens.valid()

    user = client.user()
    assert user != None and user.id != ""
    assert user != None and user.email == "admin@localhost"

    client.logout()
    assert client.tokens() == None


def test_records(trailbase: TrailBaseFixture):
    assert trailbase.isUp()

    client = connect()
    api = client.records("simple_strict_table")

    now = int(time())
    messages = [
        f"python client test 0: =?&{now}",
        f"python client test 1: =?&{now}",
    ]
    ids: List[RecordId] = []
    for msg in messages:
        ids.append(api.create({"text_not_null": msg}))

    if True:
        bulk_ids = api.create_bulk(
            [
                {"text_not_null": "python bulk test 0"},
                {"text_not_null": "python bulk test 1"},
            ]
        )
        assert len(bulk_ids) == 2

    if True:
        response = api.list(
            filters=[f"text_not_null={messages[0]}"],
        )
        records = response.records
        assert len(records) == 1
        assert records[0]["text_not_null"] == messages[0]

    if True:
        recordsAsc = api.list(
            order=["+text_not_null"],
            filters=[f"text_not_null[like]=% =?&{now}"],
            count=True,
        )

        assert recordsAsc.total_count == 2
        assert [el["text_not_null"] for el in recordsAsc.records] == messages

        recordsDesc = api.list(
            order=["-text_not_null"],
            filters=[f"text_not_null[like]=%{now}"],
        )

        assert [el["text_not_null"] for el in recordsDesc.records] == list(reversed(messages))

    if True:
        record = api.read(ids[0])
        assert record["text_not_null"] == messages[0]

        record = api.read(ids[1])
        assert record["text_not_null"] == messages[1]

    if True:
        updatedMessage = f"python client updated test 0: {now}"
        api.update(ids[0], {"text_not_null": updatedMessage})
        record = api.read(ids[0])
        assert record["text_not_null"] == updatedMessage

    if True:
        api.delete(ids[0])

        with pytest.raises(Exception):
            api.read(ids[0])


def test_expand_foreign_records(trailbase: TrailBaseFixture):
    assert trailbase.isUp()

    client = connect()
    api = client.records("comment")

    def get_nested(obj: JSON_OBJECT, k0: str, k1: str) -> JSON | None:
        x = obj[k0]
        assert type(x) is dict
        return x.get(k1)

    if True:
        comment = api.read(1)

        assert comment.get("id") == 1
        assert comment.get("body") == "first comment"
        assert get_nested(comment, "author", "id") != ""
        assert get_nested(comment, "author", "data") == None
        assert get_nested(comment, "post", "id") != ""

    if True:
        comment = api.read(1, expand=["post"])

        assert comment.get("id") == 1
        assert comment.get("body") == "first comment"
        assert get_nested(comment, "author", "data") == None

        x = get_nested(comment, "post", "data")
        assert type(x) is dict
        assert x.get("title") == "first post"

    if True:
        comments = api.list(
            expand=["author", "post"],
            order=["-id"],
            limit=1,
            count=True,
        )

        assert comments.total_count == 2
        assert len(comments.records) == 1

        comment = comments.records[0]

        assert comment.get("id") == 2
        assert comment.get("body") == "second comment"

        x = get_nested(comment, "post", "data")
        assert type(x) is dict
        assert x.get("title") == "first post"

        y = get_nested(comment, "author", "data")
        assert type(y) is dict
        assert y.get("name") == "SecondUser"


def test_subscriptions(trailbase: TrailBaseFixture):
    assert trailbase.isUp()

    client = connect()
    api = client.records("simple_strict_table")

    table_subscription = api.subscribe("*")

    now = int(time())
    create_message = f"python client test 0: =?&{now}"
    api.create({"text_not_null": create_message})

    events: List[dict[str, JSON]] = []
    for ev in table_subscription:
        events.append(ev)
        break

    table_subscription.close()

    assert "Insert" in events[0]


logger = logging.getLogger(__name__)
