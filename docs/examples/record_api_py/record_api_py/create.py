from trailbase import Client, RecordId


def create(client: Client) -> RecordId:
    return client.records("simple_strict_table").create({"text_not_null": "test"})
