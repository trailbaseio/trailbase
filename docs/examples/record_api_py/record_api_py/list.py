from trailbase import Client, Filter, CompareOp, JSON_OBJECT


def list_movies(client: Client) -> list[JSON_OBJECT]:
    response = client.records("movies").list(
        limit=3,
        order=["rank"],
        filters=[
            # Multiple filters on same column: watch_time between 90 and 120 minutes
            Filter(column="watch_time", value="90", op=CompareOp.GREATER_THAN_OR_EQUAL),
            Filter(column="watch_time", value="120", op=CompareOp.LESS_THAN),
            # Date range: movies released between 2020 and 2023
            Filter(column="release_date", value="2020-01-01", op=CompareOp.GREATER_THAN_OR_EQUAL),
            Filter(column="release_date", value="2023-12-31", op=CompareOp.LESS_THAN_OR_EQUAL),
        ],
    )

    return response.records
