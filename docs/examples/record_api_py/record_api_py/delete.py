from trailbase import Client, RecordId


def delete(client: Client, id: RecordId):
    client.records("simple_strict_table").delete(id)
