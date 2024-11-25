__title__ = "trailbase"
__description__ = "TrailBase client SDK for python."
__version__ = "0.1.0"

from time import time
from typing import TypeAlias, Any
import httpx
import jwt

JSON: TypeAlias = dict[str, "JSON"] | list["JSON"] | str | int | float | bool | None


class User:
    id: str
    email: str

    def __init__(self, id: str, email: str) -> None:
        self.id = id
        self.email = email

    @staticmethod
    def fromJson(json: dict[str, "JSON"]) -> "User":
        sub = json["sub"]
        assert isinstance(sub, str)
        email = json["email"]
        assert isinstance(email, str)

        return User(sub, email)

    def toJson(self) -> dict[str, str]:
        return {
            "sub": self.id,
            "email": self.email,
        }


class Tokens:
    auth: str
    refresh: str | None
    csrf: str | None

    def __init__(self, auth: str, refresh: str | None, csrf: str | None) -> None:
        self.auth = auth
        self.refresh = refresh
        self.csrf = csrf

    @staticmethod
    def fromJson(json: dict[str, "JSON"]) -> "Tokens":
        auth = json["auth_token"]
        assert isinstance(auth, str)
        refresh = json["refresh_token"]
        assert isinstance(refresh, str)
        csrf = json["csrf_token"]
        assert isinstance(csrf, str)

        return Tokens(auth, refresh, csrf)

    def toJson(self) -> dict[str, str | None]:
        return {
            "auth_token": self.auth,
            "refresh_token": self.refresh,
            "csrf_token": self.csrf,
        }


class JwtToken:
    sub: str
    iat: int
    exp: int
    email: str
    csrfToken: str

    def __init__(self, sub: str, iat: int, exp: int, email: str, csrfToken: str) -> None:
        self.sub = sub
        self.iat = iat
        self.exp = exp
        self.email = email
        self.csrfToken = csrfToken

    @staticmethod
    def fromJson(json: dict[str, "JSON"]) -> "JwtToken":
        sub = json["sub"]
        assert isinstance(sub, str)
        iat = json["iat"]
        assert isinstance(iat, int)
        exp = json["exp"]
        assert isinstance(exp, int)
        email = json["email"]
        assert isinstance(email, str)
        csrfToken = json["csrf_token"]
        assert isinstance(csrfToken, str)

        return JwtToken(sub, iat, exp, email, csrfToken)


class TokenState:
    state: tuple[Tokens, JwtToken] | None
    headers: dict[str, str]

    def __init__(self, state: tuple[Tokens, JwtToken] | None, headers: dict[str, str]) -> None:
        self.state = state
        self.headers = headers

    @staticmethod
    def build(tokens: Tokens | None) -> "TokenState":
        decoded = (
            jwt.decode(tokens.auth, algorithms=["EdDSA"], options={"verify_signature": False})
            if tokens != None
            else None
        )
        return TokenState(
            (tokens, JwtToken.fromJson(decoded)) if tokens != None else None, TokenState.buildHeaders(tokens)
        )

    @staticmethod
    def buildHeaders(tokens: Tokens | None) -> dict[str, str]:
        base = {
            "Content-Type": "application/json",
        }

        if tokens != None:
            base["Authorization"] = f"Bearer {tokens.auth}"

            refresh = tokens.refresh
            if refresh != None:
                base["Refresh-Token"] = refresh

            csrf = tokens.csrf
            if csrf != None:
                base["CSRF-Token"] = csrf

        return base


class ThinClient:
    http_client: httpx.Client
    site: str

    def __init__(self, site: str, http_client: httpx.Client | None = None) -> None:
        self.site = site
        self.http_client = http_client or httpx.Client()

    def fetch(
        self,
        path: str,
        tokenState: TokenState,
        method: str | None = "GET",
        data: dict[str, Any] | None = None,
        queryParams: dict[str, str] | None = None,
    ) -> httpx.Response:
        assert not path.startswith("/")

        print(f"headers: {data} {tokenState.headers}")

        return self.http_client.request(
            method=method or "GET",
            url=f"{self.site}/{path}",
            json=data,
            headers=tokenState.headers,
            params=queryParams,
        )


class Client:
    _authApi: str = "api/auth/v1"

    _client: ThinClient
    _site: str
    _tokenState: TokenState

    def __init__(
        self,
        site: str,
        tokens: Tokens | None,
        http_client: httpx.Client | None = None,
    ) -> None:
        self._client = ThinClient(site, http_client)
        self._site = site
        self._tokenState = TokenState.build(tokens)

    def tokens(self) -> Tokens | None:
        state = self._tokenState.state
        return state[0] if state else None

    def user(self) -> User | None:
        tokens = self.tokens()
        if tokens != None:
            return User.fromJson(jwt.decode(tokens.auth, algorithms=["EdDSA"], options={"verify_signature": False}))

    def site(self) -> str:
        return self._site

    def _updateTokens(self, tokens: Tokens | None):
        state = TokenState.build(tokens)

        self._tokenState = state
        # TODO: call authChange

        state = state.state
        if state != None:
            claims = state[1]

        return state

    def login(self, email: str, password: str) -> Tokens:
        response = self.fetch(
            f"{self._authApi}/login",
            method="POST",
            data={
                "email": email,
                "password": password,
            },
        )

        json = response.json()
        tokens = Tokens(
            json["auth_token"],
            json["refresh_token"],
            json["csrf_token"],
        )

        self._updateTokens(tokens)
        return tokens

    @staticmethod
    def _shouldRefresh(tokenState: TokenState) -> str | None:
        state = tokenState.state
        now = int(time())
        if state != None and state[1].exp - 60 < now:
            return state[0].refresh
        return None

    def _refreshTokensImpl(self, refreshToken: str) -> TokenState:
        response = self._client.fetch(
            f"{self._authApi}/refresh",
            self._tokenState,
            method="POST",
            data={
                "refresh_token": refreshToken,
            },
        )

        json = response.json()
        return TokenState.build(
            Tokens(
                json["auth_token"],
                refreshToken,
                json["csrf_token"],
            )
        )

    def fetch(
        self,
        path: str,
        method: str | None = "GET",
        data: dict[str, Any] | None = None,
        queryParams: dict[str, str] | None = None,
    ) -> httpx.Response:
        tokenState = self._tokenState
        refreshToken = Client._shouldRefresh(tokenState)
        if refreshToken != None:
            tokenState = self._tokenState = self._refreshTokensImpl(refreshToken)

        response = self._client.fetch(path, tokenState, method=method, data=data, queryParams=queryParams)

        return response
