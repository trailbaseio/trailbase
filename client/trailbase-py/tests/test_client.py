from re import sub
from trailbase import Client, RecordId

import httpx
import logging
import os
import pytest
import subprocess

from time import time, sleep

logging.basicConfig(level=logging.DEBUG)

port = 4007
address = f"127.0.0.1:{port}"
site = f"http://{address}"


class TrailBaseFixture:
    process: None | subprocess.Popen

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
    assert tokens != None and tokens.isValid()

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
        f"dart client test 0: =?&{now}",
        f"dart client test 1: =?&{now}",
    ]
    ids: list[RecordId] = []
    for msg in messages:
        ids.append(api.create({"text_not_null": msg}))

    if True:
        records = api.list(
            filters=[f"text_not_null={messages[0]}"],
        )
        assert len(records) == 1
        assert records[0]["text_not_null"] == messages[0]

    if True:
        recordsAsc = api.list(
            order=["+text_not_null"],
            filters=[f"text_not_null[like]=% =?&{now}"],
        )

        assert [el["text_not_null"] for el in recordsAsc] == messages

        recordsDesc = api.list(
            order=["-text_not_null"],
            filters=[f"text_not_null[like]=%{now}"],
        )

        assert [el["text_not_null"] for el in recordsDesc] == list(reversed(messages))

    if True:
        record = api.read(ids[0])
        assert record["text_not_null"] == messages[0]

        record = api.read(ids[1])
        assert record["text_not_null"] == messages[1]

    if True:
        updatedMessage = f"dart client updated test 0: {now}"
        api.update(ids[0], {"text_not_null": updatedMessage})
        record = api.read(ids[0])
        assert record["text_not_null"] == updatedMessage

    if True:
        api.delete(ids[0])

        with pytest.raises(Exception):
            api.read(ids[0])


logger = logging.getLogger(__name__)
