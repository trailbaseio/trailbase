from trailbase import Client, RecordId, JSON_OBJECT


def read(client: Client, id: RecordId) -> JSON_OBJECT:
    return client.records("simple_strict_table").read(id)
