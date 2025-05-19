from trailbase import Client, Filter, CompareOp, JSON_OBJECT


def list_movies(client: Client) -> list[JSON_OBJECT]:
    response = client.records("movies").list(
        limit=3,
        order=["rank"],
        filters=[
            Filter(column="watch_time", value="120", op=CompareOp.LESS_THAN),
            Filter(column="description", value="%love%", op=CompareOp.LIKE),
        ],
    )

    return response.records
