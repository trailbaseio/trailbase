__title__ = "trailbase"
__description__ = "TrailBase client SDK for python."
__version__ = "0.1.0"

__all__ = [
    "Client",
    "CompareOp",
    "Filter",
    "And",
    "Or",
    "RecordId",
    "User",
    "ListResponse",
    "Tokens",
    "JSON",
    "JSON_OBJECT",
    "JSON_ARRAY",
    "TransactionBatch",
    "ApiBatch",
]

import httpx
import jwt
import logging
import typing
import json

from enum import Enum
from contextlib import contextmanager
from time import time
from typing import TypeAlias, List, Protocol, cast, final

JSON: TypeAlias = dict[str, "JSON"] | list["JSON"] | str | int | float | bool | None
JSON_OBJECT: TypeAlias = dict[str, JSON]
JSON_ARRAY: TypeAlias = list[JSON]


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
    email: str

    def __init__(self, id: str, email: str) -> None:
        self.id = id
        self.email = email

    @staticmethod
    def from_json(json: JSON_OBJECT) -> "User":
        sub = json["sub"]
        assert isinstance(sub, str)
        email = json["email"]
        assert isinstance(email, str)

        return User(sub, email)

    def to_json(self) -> dict[str, str]:
        return {
            "sub": self.id,
            "email": self.email,
        }


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
        refresh = json["refresh_token"]
        assert isinstance(refresh, str)
        csrf = json["csrf_token"]
        assert isinstance(csrf, str)

        return Tokens(auth, refresh, csrf)

    def to_json(self) -> dict[str, str | None]:
        return {
            "auth_token": self.auth,
            "refresh_token": self.refresh,
            "csrf_token": self.csrf,
        }

    def valid(self) -> bool:
        return jwt.decode(self.auth, algorithms=["EdDSA"], options={"verify_signature": False}) != None


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
    def from_json(json: JSON_OBJECT) -> "JwtToken":
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

        if decoded == None or tokens == None:
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
        data: JSON | None = None,
        queryParams: dict[str, str] | None = None,
    ) -> httpx.Response:
        assert not path.startswith("/")
        return self.http_client.request(
            method=method or "GET",
            url=f"{self.site}/{path}",
            json=data,
            headers=tokenState.headers,
            params=queryParams,
        )

    def stream(
        self,
        path: str,
        tokenState: TokenState,
        method: str | None = "GET",
        queryParams: dict[str, str] | None = None,
        timeout: httpx.Timeout | None = None,
    ):
        assert not path.startswith("/")
        headers = tokenState.headers.copy()
        headers["Accept"] = "text/event-stream"
        headers["Cache-Control"] = "no-store"

        request = self.http_client.build_request(
            method=method or "GET",
            url=f"{self.site}/{path}",
            headers=headers,
            params=queryParams,
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


# Transaction related classes and protocols
class CreateOperation(typing.TypedDict):
    api_name: str
    value: JSON_OBJECT


class UpdateOperation(typing.TypedDict):
    api_name: str
    record_id: str
    value: JSON_OBJECT


class DeleteOperation(typing.TypedDict):
    api_name: str
    record_id: str


class Operation(typing.TypedDict, total=False):
    Create: CreateOperation
    Update: UpdateOperation
    Delete: DeleteOperation


class TransactionRequest(typing.TypedDict):
    operations: List[Operation]


class TransactionResponse(typing.TypedDict):
    ids: List[str]


class ITransactionBatch(Protocol):
    def api(self, api_name: str) -> "IApiBatch": ...

    def send(self) -> List[RecordId]: ...


class IApiBatch(Protocol):
    def create(self, value: JSON_OBJECT) -> ITransactionBatch: ...

    def update(self, recordId: RecordId | str | int, value: JSON_OBJECT) -> ITransactionBatch: ...

    def delete(self, recordId: RecordId | str | int) -> ITransactionBatch: ...


class TransactionBatch:
    _transactionApi: str = "api/transaction/v1/execute"

    _client: "Client"
    _operations: List[Operation]

    def __init__(self, client: "Client") -> None:
        self._client = client
        self._operations = []

    def api(self, api_name: str) -> "ApiBatch":
        return ApiBatch(self, api_name)

    def send(self) -> List[RecordId]:
        ops_json = [dict(op) for op in self._operations]

        response = self._client.fetch(
            self._transactionApi,
            method="POST",
            data=cast(JSON, {"operations": ops_json}),
        )
        if response.status_code != 200:
            raise Exception(f"Transaction failed with status code {response.status_code}: {response.text}")

        return record_ids_from_json(response.json())

    def add_operation(self, operation: Operation) -> None:
        self._operations.append(operation)


class ApiBatch:
    _batch: TransactionBatch
    _api_name: str

    def __init__(self, batch: TransactionBatch, api_name: str) -> None:
        self._batch = batch
        self._api_name = api_name

    def create(self, value: JSON_OBJECT) -> ITransactionBatch:
        operation: Operation = {"Create": {"api_name": self._api_name, "value": value}}
        self._batch.add_operation(operation)
        return self._batch

    def update(self, recordId: RecordId | str | int, value: JSON_OBJECT) -> ITransactionBatch:
        id = repr(recordId) if isinstance(recordId, RecordId) else f"{recordId}"
        operation: Operation = {"Update": {"api_name": self._api_name, "record_id": id, "value": value}}
        self._batch.add_operation(operation)
        return self._batch

    def delete(self, recordId: RecordId | str | int) -> ITransactionBatch:
        id = repr(recordId) if isinstance(recordId, RecordId) else f"{recordId}"
        operation: Operation = {"Delete": {"api_name": self._api_name, "record_id": id}}
        self._batch.add_operation(operation)
        return self._batch


class Client:
    _authApi: str = "api/auth/v1"

    _client: ThinClient
    _site: str
    _tokenState: TokenState

    def __init__(
        self,
        site: str,
        tokens: Tokens | None = None,
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
            return User.from_json(
                jwt.decode(
                    tokens.auth,
                    algorithms=["EdDSA"],
                    options={"verify_signature": False},
                )
            )

    def site(self) -> str:
        return self._site

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

    def logout(self) -> None:
        state = self._tokenState.state
        refreshToken = state[0].refresh if state else None
        try:
            if refreshToken != None:
                self.fetch(
                    f"{self._authApi}/logout",
                    method="POST",
                    data={
                        "refresh_token": refreshToken,
                    },
                )
            else:
                self.fetch(f"{self._authApi}/logout")
        except:
            pass

        self._updateTokens(None)

    def records(self, name: str) -> "RecordApi":
        return RecordApi(name, self)

    def transaction(self) -> TransactionBatch:
        return TransactionBatch(self)

    def _updateTokens(self, tokens: Tokens | None):
        state = TokenState.build(tokens)

        self._tokenState = state

        state = state.state
        if state != None:
            claims = state[1]
            now = int(time())
            if claims.exp < now:
                logger.warning("Token expired")

        return state

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
        data: JSON | None = None,
        queryParams: dict[str, str] | None = None,
    ) -> httpx.Response:
        tokenState = self._tokenState
        refreshToken = Client._shouldRefresh(tokenState)
        if refreshToken != None:
            tokenState = self._tokenState = self._refreshTokensImpl(refreshToken)

        return self._client.fetch(path, tokenState, method=method, data=data, queryParams=queryParams)

    def stream(
        self,
        path: str,
        method: str | None = "GET",
        queryParams: dict[str, str] | None = None,
        timeout: httpx.Timeout | None = None,
    ):
        tokenState = self._tokenState
        refreshToken = Client._shouldRefresh(tokenState)
        if refreshToken != None:
            tokenState = self._tokenState = self._refreshTokensImpl(refreshToken)

        return self._client.stream(
            path,
            tokenState,
            method=method,
            queryParams=queryParams,
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


@final
class Filter:
    column: str
    op: CompareOp | None
    value: str

    def __init__(self, column: str, value: str, op: CompareOp | None = None):
        self.column = column
        self.op = op
        self.value = value


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

        if cursor != None:
            params["cursor"] = cursor

        if limit != None:
            params["limit"] = str(limit)

        if offset != None:
            params["offset"] = str(offset)

        if order != None:
            params["order"] = ",".join(order)

        if expand != None:
            params["expand"] = ",".join(expand)

        if count:
            params["count"] = "true"

        def traverseFilters(path: str, filter: FilterOrComposite):
            match filter:
                case Filter() as f:
                    if f.op != None:
                        params[f"{path}[{f.column}][{repr(f.op)}]"] = f.value
                    else:
                        params[f"{path}[{f.column}]"] = f.value
                case And() as f:
                    for i, filter in enumerate(f.filters):
                        traverseFilters(f"{path}[$and][{i}", filter)
                case Or() as f:
                    for i, filter in enumerate(f.filters):
                        traverseFilters(f"{path}[$or][{i}", filter)

        if filters != None:
            for filter in filters:
                traverseFilters("filter", filter)

        response = self._client.fetch(f"{self._recordApi}/{self._name}", queryParams=params)
        return ListResponse.from_json(response.json())

    def read(
        self,
        recordId: RecordId | str | int,
        expand: "list[str] | None" = None,
    ) -> JSON_OBJECT:
        id = repr(recordId) if isinstance(recordId, RecordId) else f"{recordId}"
        params = {"expand": ",".join(expand)} if expand != None else None

        return self._client.fetch(
            f"{self._recordApi}/{self._name}/{id}",
            queryParams=params,
        ).json()

    def create(self, record: JSON_OBJECT) -> RecordId:
        response = self._client.fetch(
            f"{self._recordApi}/{self._name}",
            method="POST",
            data=record,
        )
        if response.status_code > 200:
            raise Exception(f"{response}")

        return record_ids_from_json(response.json())[0]

    def create_bulk(self, records: JSON_ARRAY):
        response = self._client.fetch(
            f"{self._recordApi}/{self._name}",
            method="POST",
            data=records,
        )
        if response.status_code > 200:
            raise Exception(f"{response}")

        return record_ids_from_json(response.json())

    def update(self, recordId: RecordId | str | int, record: JSON_OBJECT) -> None:
        id = repr(recordId) if isinstance(recordId, RecordId) else f"{recordId}"
        response = self._client.fetch(
            f"{self._recordApi}/{self._name}/{id}",
            method="PATCH",
            data=record,
        )
        if response.status_code > 200:
            raise Exception(f"{response}")

    def delete(self, recordId: RecordId | str | int) -> None:
        id = repr(recordId) if isinstance(recordId, RecordId) else f"{recordId}"
        response = self._client.fetch(
            f"{self._recordApi}/{self._name}/{id}",
            method="DELETE",
        )
        if response.status_code > 200:
            raise Exception(f"{response}")

    def subscribe(self, recordId: RecordId | str | int) -> typing.Generator[JSON_OBJECT]:
        id = repr(recordId) if isinstance(recordId, RecordId) else f"{recordId}"
        context = self._client.stream(
            f"{self._recordApi}/{self._name}/subscribe/{id}", timeout=httpx.Timeout(None)
        )

        def impl() -> typing.Generator[JSON_OBJECT]:
            with context as response:
                if response.status_code > 200:
                    raise Exception(f"{response}")

                for line in response.iter_lines():
                    if line.startswith("data: "):
                        yield json.loads(line.rstrip("\n")[6:])

        return impl()


logger = logging.getLogger(__name__)
