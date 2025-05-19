from trailbase import Client, RecordId


def update(client: Client, id: RecordId):
    client.records("simple_strict_table").update(
        id,
        {
            "text_not_null": "updated",
        },
    )
