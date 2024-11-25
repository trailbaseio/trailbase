from trailbase import Client


# @pytest.mark.asyncio
def test_client_login():
    site = "http://localhost:4000"
    client = Client(site, tokens=None)
    assert client.site() == site

    client.login("admin@localhost", "secret")
    user = client.user()
    assert user != None and user.email == "admin@localhost"
