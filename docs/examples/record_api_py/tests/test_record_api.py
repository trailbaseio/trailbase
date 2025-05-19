from trailbase import Client
from record_api_py import create, delete, list_movies, read, update


def test_record_api():
    client = Client("http://localhost:4000")
    client.login("admin@localhost", "secret")

    id = create(client)
    json = read(client, id)

    assert json["text_not_null"] == "test"

    update(client, id)
    updated_json = read(client, id)

    assert updated_json["text_not_null"] == "updated"

    delete(client, id)

    movies = list_movies(client)
    assert len(movies) == 3
