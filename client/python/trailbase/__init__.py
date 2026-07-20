__title__ = "trailbase"
__description__ = "TrailBase client SDK for python."
__version__ = "0.1.0"

import httpx
import jwt
import logging
import typing
import json

from abc import ABC, abstractmethod
from enum import Enum
from contextlib import contextmanager
from time import time
from typing import ContextManager, TypeAlias, cast, final

JSON: TypeAlias = dict[str, "JSON"] | list["JSON"] | str | int | float | bool | None
JSON_OBJECT: TypeAlias = dict[str, JSON]
JSON_ARRAY: TypeAlias = list[JSON]


class FetchException(Exception):
    status: int
    message: str

    def __init__(self, status: int, message: str):
        self.status = status
        self.message = message
        super().__init__(f"FetchException(status={self.status}, '{self.message}')")


class RecordId:
    id: str

    def __init__(self, id: str):
        self.id = id

    def __repr__(self) -> str:
        return f"{self.id}"


def record_ids_from_json(json: JSON_OBJECT) -> list[RecordId]:
    ids = json["ids"]
    assert isinstance(ids, list)

    def convert(value: JSON) -> RecordId:
        assert isinstance(value, str)
        return RecordId(value)

    return list([convert(id) for id in ids])


class User:
    id: str
    email: str | None
    username: str | None

    def __init__(self, id: str, email: str | None, username: str | None) -> None:
        self.id = id
        self.email = email
        self.username = username

    @staticmethod
    def from_json(json: JSON_OBJECT) -> "User":
        sub = json["sub"]
        assert isinstance(sub, str)
        email = json["email"]
        assert isinstance(email, str | None)
        username = json["username"]
        assert isinstance(username, str | None)

        return User(sub, email, username)


class ListResponse:
    cursor: str | None
    total_count: int | None
    records: list[JSON_OBJECT]

    def __init__(self, cursor: str | None, total_count: int | None, records: list[JSON_OBJECT]) -> None:
        self.cursor = cursor
        self.total_count = total_count
        self.records = records

    @staticmethod
    def from_json(json: JSON_OBJECT) -> "ListResponse":
        cursor = json.get("cursor")
        assert isinstance(cursor, str | None)
        total_count = json.get("total_count")
        assert isinstance(total_count, int | None)
        records = json["records"]
        assert isinstance(records, list)

        return ListResponse(cursor, total_count, cast(list[JSON_OBJECT], records))


class Tokens:
    auth: str
    refresh: str | None
    csrf: str | None

    def __init__(self, auth: str, refresh: str | None, csrf: str | None) -> None:
        self.auth = auth
        self.refresh = refresh
        self.csrf = csrf

    @staticmethod
    def from_json(json: JSON_OBJECT) -> "Tokens":
        auth = json["auth_token"]
        assert isinstance(auth, str)
        refresh = json.get("refresh_token")
        assert isinstance(refresh, str | None)
        csrf = json.get("csrf_token")
        assert isinstance(csrf, str | None)

        return Tokens(auth, refresh, csrf)

    def valid(self) -> bool:
        claims = jwt.decode(self.auth, algorithms=["EdDSA"], options={"verify_signature": False})
        return len(claims) > 0


class MultiFactorAuthToken:
    token: str

    def __init__(self, token: str) -> None:
        self.token = token

    @staticmethod
    def from_json(json: JSON_OBJECT) -> "MultiFactorAuthToken":
        token = json["mfa_token"]
        assert isinstance(token, str)
        return MultiFactorAuthToken(token)


class JwtToken:
    sub: str
    iat: int
    exp: int
    email: str | None
    username: str | None
    csrfToken: str

    def __init__(
        self, sub: str, iat: int, exp: int, email: str | None, username: str | None, csrfToken: str
    ) -> None:
        self.sub = sub
        self.iat = iat
        self.exp = exp
        self.email = email
        self.username = username
        self.csrfToken = csrfToken

    @staticmethod
    def from_json(json: JSON_OBJECT) -> "JwtToken":
        sub = json["sub"]
        assert isinstance(sub, str)
        iat = json["iat"]
        assert isinstance(iat, int)
        exp = json["exp"]
        assert isinstance(exp, int)
        email = json["email"]
        assert isinstance(email, str | None)
        username = json["username"]
        assert isinstance(username, str | None)
        csrf_token = json["csrf_token"]
        assert isinstance(csrf_token, str)

        return JwtToken(sub, iat, exp, email, username, csrf_token)


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
            if tokens is not None
            else None
        )

        if decoded is None or tokens is None:
            return TokenState(None, TokenState.build_headers(tokens))

        return TokenState(
            (tokens, JwtToken.from_json(decoded)),
            TokenState.build_headers(tokens),
        )

    @staticmethod
    def build_headers(tokens: Tokens | None) -> dict[str, str]:
        base = {
            "Content-Type": "application/json",
        }

        if tokens is not None:
            base["Authorization"] = f"Bearer {tokens.auth}"

            refresh = tokens.refresh
            if refresh is not None:
                base["Refresh-Token"] = refresh

            csrf = tokens.csrf
            if csrf is not None:
                base["CSRF-Token"] = csrf

        return base


class Event:
    seq: int | None

    def __init__(self, seq: int | None):
        self.seq = seq


class InsertEvent(Event):
    value: JSON_OBJECT

    def __init__(self, seq: int | None, value: JSON_OBJECT):
        super().__init__(seq)
        self.value = value


class UpdateEvent(Event):
    value: JSON_OBJECT

    def __init__(self, seq: int | None, value: JSON_OBJECT):
        super().__init__(seq)
        self.value = value


class DeleteEvent(Event):
    value: JSON_OBJECT

    def __init__(self, seq: int | None, value: JSON_OBJECT):
        super().__init__(seq)
        self.value = value


class ErrorEvent(Event):
    status: int
    message: str | None

    def __init__(self, seq: int | None, status: int, message: str | None):
        super().__init__(seq)
        self.status = status
        self.message = message


EVENT_ERROR_STATUS_UNKNOWN = 0
EVENT_ERROR_STATUS_FORBIDDEN = 1
EVENT_ERROR_STATUS_LOSS = 2


EVENT: TypeAlias = UpdateEvent | InsertEvent | DeleteEvent | ErrorEvent


def parseEvent(obj: JSON_OBJECT) -> EVENT | None:
    seq = cast(int | None, obj["seq"])

    insert = obj.get("Insert")
    if insert is not None:
        return InsertEvent(seq, cast(JSON_OBJECT, insert))

    update = obj.get("Update")
    if update is not None:
        return UpdateEvent(seq, cast(JSON_OBJECT, update))

    delete = obj.get("Delete")
    if delete is not None:
        return DeleteEvent(seq, cast(JSON_OBJECT, delete))

    error = cast(JSON_OBJECT | None, obj.get("Error"))
    if error is not None:
        return ErrorEvent(seq, cast(int, error["status"]), cast(str | None, error.get("message")))

    raise Exception(f"Failed to parse event: {obj}")


class Transport(ABC):
    @abstractmethod
    def fetch(
        self,
        path: str,
        method: str | None = "GET",
        headers: dict[str, str] | None = None,
        query_params: dict[str, str] | None = None,
        body: JSON | None = None,
    ) -> httpx.Response:
        pass

    @abstractmethod
    def stream(
        self,
        path: str,
        method: str | None = "GET",
        headers: dict[str, str] | None = None,
        query_params: dict[str, str] | None = None,
        timeout: httpx.Timeout | None = None,
    ) -> ContextManager[httpx.Response]:
        pass


class DefaultTransport(Transport):
    http_client: httpx.Client
    site: str

    def __init__(self, site: str, http_client: httpx.Client | None = None) -> None:
        self.site = site
        self.http_client = http_client or httpx.Client()

    def fetch(
        self,
        path: str,
        method: str | None = "GET",
        headers: dict[str, str] | None = None,
        query_params: dict[str, str] | None = None,
        body: JSON | None = None,
    ) -> httpx.Response:
        assert not path.startswith("/")
        return self.http_client.request(
            method=method or "GET",
            url=f"{self.site}/{path}",
            json=body,
            headers=headers,
            params=query_params,
        )

    def stream(
        self,
        path: str,
        method: str | None = "GET",
        headers: dict[str, str] | None = None,
        query_params: dict[str, str] | None = None,
        timeout: httpx.Timeout | None = None,
    ) -> ContextManager[httpx.Response]:
        assert not path.startswith("/")
        headers = (headers or {}).copy()
        headers["Accept"] = "text/event-stream"
        headers["Cache-Control"] = "no-store"

        request = self.http_client.build_request(
            method=method or "GET",
            url=f"{self.site}/{path}",
            headers=headers,
            params=query_params,
            timeout=timeout,
        )

        response = self.http_client.send(
            request=request,
            stream=True,
        )

        @contextmanager
        def impl():
            try:
                yield response
            finally:
                response.close()

        return impl()


class Client:
    _transport: Transport
    _site: str
    _token_state: TokenState

    def __init__(
        self,
        site: str,
        tokens: Tokens | None = None,
        transport: Transport | None = None,
    ) -> None:
        self._transport = transport or DefaultTransport(site)
        self._site = site
        self._token_state = TokenState.build(tokens)

    def tokens(self) -> Tokens | None:
        state = self._token_state.state
        return state[0] if state else None

    def user(self) -> User | None:
        tokens = self.tokens()
        if tokens is not None:
            return User.from_json(
                jwt.decode(
                    tokens.auth,
                    algorithms=["EdDSA"],
                    options={"verify_signature": False},
                )
            )

    def site(self) -> str:
        return self._site

    def login(self, email_or_username: str, password: str) -> MultiFactorAuthToken | None:
        response = self.fetch(
            f"{_AUTH_API}/login",
            method="POST",
            data={
                "email_or_username": email_or_username,
                "password": password,
            },
            throwOnError=False,
        )

        if response.status_code == 403:
            return MultiFactorAuthToken.from_json(response.json())
        elif response.status_code > 200:
            raise FetchException(response.status_code, response.text)

        self._set_token_state(TokenState.build(Tokens.from_json(response.json())))

        return None

    def login_second(self, token: MultiFactorAuthToken, code: str) -> None:
        response = self.fetch(
            f"{_AUTH_API}/login_mfa",
            method="POST",
            data={
                "mfa_token": token.token,
                "totp": code,
            },
        )

        self._set_token_state(TokenState.build(Tokens.from_json(response.json())))

    def request_otp(self, email_or_username: str) -> None:
        self.fetch(
            f"{_AUTH_API}/otp/request",
            method="POST",
            data={
                "email_or_username": email_or_username,
                # "redirect_uri": redirect_uri,
            },
        )

    def login_otp(self, email: str, code: str) -> None:
        response = self.fetch(
            f"{_AUTH_API}/otp/login",
            method="POST",
            data={
                "email": email,
                "code": code,
            },
        )

        self._set_token_state(TokenState.build(Tokens.from_json(response.json())))

    def login_anonymously(self) -> None:
        response = self.fetch(
            f"{_AUTH_API}/login_anonymous",
            method="POST",
            data={},
        )

        self._set_token_state(TokenState.build(Tokens.from_json(response.json())))

    def logout(self) -> None:
        state = self._token_state.state
        refresh_token = state[0].refresh if state else None

        try:
            if refresh_token is not None:
                self.fetch(
                    f"{_AUTH_API}/logout",
                    method="POST",
                    data={
                        "refresh_token": refresh_token,
                    },
                )
            else:
                self.fetch(f"{_AUTH_API}/logout")
        finally:
            self._set_token_state(TokenState.build(None))

    def promote_anonymous(
        self, password: str, email: str | None = None, username: str | None = None
    ) -> None:
        self.fetch(
            f"{_AUTH_API}/login_anonymous",
            method="POST",
            data={
                "new_password": password,
                "new_email": email,
                "new_username": username,
            },
        )

    def records(self, name: str) -> "RecordApi":
        return RecordApi(name, self)

    def refresh_auth_tokens(self, force: bool = False) -> bool:
        state = self._token_state.state
        refresh_token = (
            state[0].refresh if force and state is not None else Client._should_refresh(self._token_state)
        )

        if refresh_token is not None:
            self._set_token_state(_refresh_tokens_impl(self._transport, refresh_token))
            return True

        return False

    def _set_token_state(self, token_state: TokenState) -> TokenState:
        self._token_state = token_state

        state = token_state.state
        if state is not None:
            claims = state[1]
            now = int(time())
            if claims.exp < now:
                _logger.warning("Token expired")

        return token_state

    @staticmethod
    def _should_refresh(token_state: TokenState) -> str | None:
        state = token_state.state
        now = int(time())
        if state is not None and state[1].exp - 60 < now:
            return state[0].refresh
        return None

    def fetch(
        self,
        path: str,
        method: str | None = "GET",
        data: JSON | None = None,
        query_params: dict[str, str] | None = None,
        throwOnError: bool = True,
    ) -> httpx.Response:
        token_state = self._token_state
        refresh_token = Client._should_refresh(token_state)
        if refresh_token is not None:
            token_state = self._set_token_state(_refresh_tokens_impl(self._transport, refresh_token))

        response = self._transport.fetch(
            path, method=method, headers=token_state.headers, query_params=query_params, body=data
        )

        if response.status_code > 200 and throwOnError:
            raise FetchException(response.status_code, response.text)

        return response

    def stream(
        self,
        path: str,
        method: str | None = "GET",
        query_params: dict[str, str] | None = None,
        timeout: httpx.Timeout | None = None,
    ):
        token_state = self._token_state
        refresh_token = Client._should_refresh(token_state)
        if refresh_token is not None:
            token_state = self._set_token_state(_refresh_tokens_impl(self._transport, refresh_token))

        return self._transport.stream(
            path,
            method=method,
            headers=token_state.headers,
            query_params=query_params,
            timeout=timeout,
        )


class CompareOp(Enum):
    EQUAL = 1
    NOT_EQUAL = 2
    LESS_THAN = 3
    LESS_THAN_EQUAL = 4
    GREATER_THAN = 5
    GREATER_THAN_EQUAL = 6
    LIKE = 7
    REGEXP = 8
    ST_WITHIN = 9
    ST_INTERSECTS = 10
    ST_CONTAINS = 11
    IS_NULL = 12
    IS_NOT_NULL = 13

    def __repr__(self) -> str:
        match self:
            case CompareOp.EQUAL:
                return "$eq"
            case CompareOp.NOT_EQUAL:
                return "$ne"
            case CompareOp.LESS_THAN:
                return "$lt"
            case CompareOp.LESS_THAN_EQUAL:
                return "$lte"
            case CompareOp.GREATER_THAN:
                return "$gt"
            case CompareOp.GREATER_THAN_EQUAL:
                return "$gte"
            case CompareOp.LIKE:
                return "$like"
            case CompareOp.REGEXP:
                return "$re"
            case CompareOp.ST_WITHIN:
                return "@within"
            case CompareOp.ST_INTERSECTS:
                return "@intersects"
            case CompareOp.ST_CONTAINS:
                return "@contains"
            case CompareOp.IS_NULL:
                return "$is"
            case CompareOp.IS_NOT_NULL:
                return "$is"


@final
class Filter:
    column: str
    op: CompareOp | None
    value: str

    def __init__(self, column: str, value: str, op: CompareOp | None = None):
        self.column = column
        self.op = op
        self.value = value

    @classmethod
    def is_null(cls, column: str) -> "Filter":
        """Filter rows where `column` IS NULL.

        Wire format: ``filter[<column>][$is]=NULL``.
        """
        return cls(column=column, value="NULL", op=CompareOp.IS_NULL)

    @classmethod
    def is_not_null(cls, column: str) -> "Filter":
        """Filter rows where `column` IS NOT NULL.

        Wire format: ``filter[<column>][$is]=!NULL``.
        """
        return cls(column=column, value="!NULL", op=CompareOp.IS_NOT_NULL)


@final
class And:
    filters: list["FilterOrComposite"]

    def __init__(self, filters: list["FilterOrComposite"]):
        self.filters = filters


@final
class Or:
    filters: list["FilterOrComposite"]

    def __init__(self, filters: list["FilterOrComposite"]):
        self.filters = filters


FilterOrComposite: TypeAlias = Filter | And | Or


class RecordApi:
    _recordApi: str = "api/records/v1"

    _name: str
    _client: Client

    def __init__(self, name: str, client: Client) -> None:
        self._name = name
        self._client = client

    def list(
        self,
        order: list[str] | None = None,
        filters: list[FilterOrComposite] | None = None,
        cursor: str | None = None,
        expand: list[str] | None = None,
        limit: int | None = None,
        offset: int | None = None,
        count: bool = False,
    ) -> ListResponse:
        params: dict[str, str] = {}

        if cursor is not None:
            params["cursor"] = cursor

        if limit is not None:
            params["limit"] = str(limit)

        if offset is not None:
            params["offset"] = str(offset)

        if order is not None:
            params["order"] = ",".join(order)

        if expand is not None:
            params["expand"] = ",".join(expand)

        if count:
            params["count"] = "true"

        def traverse_filters(path: str, filter: FilterOrComposite):
            match filter:
                case Filter() as f:
                    if f.op is not None:
                        params[f"{path}[{f.column}][{repr(f.op)}]"] = f.value
                    else:
                        params[f"{path}[{f.column}]"] = f.value
                case And() as f:
                    for i, filter in enumerate(f.filters):
                        traverse_filters(f"{path}[$and][{i}", filter)
                case Or() as f:
                    for i, filter in enumerate(f.filters):
                        traverse_filters(f"{path}[$or][{i}", filter)

        if filters is not None:
            for filter in filters:
                traverse_filters("filter", filter)

        response = self._client.fetch(f"{self._recordApi}/{self._name}", query_params=params)
        return ListResponse.from_json(response.json())

    def read(
        self,
        record_id: RecordId | str | int,
        expand: "list[str] | None" = None,
    ) -> JSON_OBJECT:
        id = repr(record_id) if isinstance(record_id, RecordId) else f"{record_id}"
        params = {"expand": ",".join(expand)} if expand is not None else None

        return self._client.fetch(
            f"{self._recordApi}/{self._name}/{id}",
            query_params=params,
        ).json()

    def create(self, record: JSON_OBJECT) -> RecordId:
        response = self._client.fetch(
            f"{self._recordApi}/{self._name}",
            method="POST",
            data=record,
        )
        return record_ids_from_json(response.json())[0]

    def create_bulk(self, records: JSON_ARRAY):
        response = self._client.fetch(
            f"{self._recordApi}/{self._name}",
            method="POST",
            data=records,
        )
        return record_ids_from_json(response.json())

    def update(self, record_id: RecordId | str | int, record: JSON_OBJECT) -> None:
        id = repr(record_id) if isinstance(record_id, RecordId) else f"{record_id}"
        self._client.fetch(
            f"{self._recordApi}/{self._name}/{id}",
            method="PATCH",
            data=record,
        )

    def delete(self, record_id: RecordId | str | int) -> None:
        id = repr(record_id) if isinstance(record_id, RecordId) else f"{record_id}"
        self._client.fetch(
            f"{self._recordApi}/{self._name}/{id}",
            method="DELETE",
        )

    def subscribe(self, record_id: RecordId | str | int) -> typing.Generator[EVENT]:
        id = repr(record_id) if isinstance(record_id, RecordId) else f"{record_id}"
        context = self._client.stream(
            f"{self._recordApi}/{self._name}/subscribe/{id}", timeout=httpx.Timeout(None)
        )

        def impl() -> typing.Generator[EVENT]:
            with context as response:
                if response.status_code > 200:
                    raise FetchException(response.status_code, response.text)

                for line in response.iter_lines():
                    if line.startswith("data: "):
                        ev = parseEvent(json.loads(line.rstrip("\n")[6:]))
                        if ev is not None:
                            yield ev

        return impl()

    def subscribe_all(self) -> typing.Generator[EVENT]:
        return self.subscribe("*")


def _refresh_tokens_impl(transport: Transport, refresh_token: str) -> TokenState:
    response = transport.fetch(
        f"{_AUTH_API}/refresh",
        method="POST",
        headers={
            "Content-Type": "application/json",
        },
        body={
            "refresh_token": refresh_token,
        },
    )

    match response.status_code:
        case 200:
            from_json = Tokens.from_json(response.json())
            return TokenState.build(Tokens(from_json.auth, refresh_token, from_json.csrf))
        case 401:
            # Refresh token was rejected w/o means to recover. May as well log out.
            return TokenState.build(None)
        case _:
            raise FetchException(response.status_code, response.text)


_logger = logging.getLogger(__name__)
_AUTH_API: str = "api/auth/v1"
