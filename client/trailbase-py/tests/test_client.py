from trailbase import Client, RecordId

from time import time

site = "http://localhost:4000"


def connect() -> Client:
    client = Client(site, tokens=None)
    client.login("admin@localhost", "secret")
    return client


def test_client_login():
    client = connect()
    assert client.site() == site

    user = client.user()
    assert user != None and user.email == "admin@localhost"

    client.logout()
    assert client.tokens() == None


def test_records():
    client = connect()
    api = client.records("simple_strict_table")

    now = int(time())
    messages = [
        f"dart client test 0: {now}",
        f"dart client test 1: {now}",
    ]
    ids: list[RecordId] = []
    for msg in messages:
        ids.append(api.create({"text_not_null": msg}))

    records = api.list(
        filters=[f"text_not_null={messages[0]}"],
    )
    assert len(records) == 1
    assert records[0]["text_not_null"] == messages[0]
