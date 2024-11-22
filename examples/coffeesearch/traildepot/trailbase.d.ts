export type HeaderMapType = {
    [key: string]: string;
};
export type PathParamsType = {
    [key: string]: string;
};
export type UserType = {
    id: string;
    email: string;
    csrf: string;
};
export type RequestType = {
    uri: string;
    params: PathParamsType;
    headers: HeaderMapType;
    user?: UserType;
    body?: Uint8Array;
};
export type ResponseType = {
    headers?: [string, string][];
    status?: number;
    body?: Uint8Array;
};
export type MaybeResponse<T> = Promise<T | undefined> | T | undefined;
export type CallbackType = (req: RequestType) => MaybeResponse<ResponseType>;
export type Method = "DELETE" | "GET" | "HEAD" | "OPTIONS" | "PATCH" | "POST" | "PUT" | "TRACE";
export declare enum StatusCodes {
    CONTINUE = 100,
    SWITCHING_PROTOCOLS = 101,
    PROCESSING = 102,
    EARLY_HINTS = 103,
    OK = 200,
    CREATED = 201,
    ACCEPTED = 202,
    NON_AUTHORITATIVE_INFORMATION = 203,
    NO_CONTENT = 204,
    RESET_CONTENT = 205,
    PARTIAL_CONTENT = 206,
    MULTI_STATUS = 207,
    MULTIPLE_CHOICES = 300,
    MOVED_PERMANENTLY = 301,
    MOVED_TEMPORARILY = 302,
    SEE_OTHER = 303,
    NOT_MODIFIED = 304,
    USE_PROXY = 305,
    TEMPORARY_REDIRECT = 307,
    PERMANENT_REDIRECT = 308,
    BAD_REQUEST = 400,
    UNAUTHORIZED = 401,
    PAYMENT_REQUIRED = 402,
    FORBIDDEN = 403,
    NOT_FOUND = 404,
    METHOD_NOT_ALLOWED = 405,
    NOT_ACCEPTABLE = 406,
    PROXY_AUTHENTICATION_REQUIRED = 407,
    REQUEST_TIMEOUT = 408,
    CONFLICT = 409,
    GONE = 410,
    LENGTH_REQUIRED = 411,
    PRECONDITION_FAILED = 412,
    REQUEST_TOO_LONG = 413,
    REQUEST_URI_TOO_LONG = 414,
    UNSUPPORTED_MEDIA_TYPE = 415,
    REQUESTED_RANGE_NOT_SATISFIABLE = 416,
    EXPECTATION_FAILED = 417,
    IM_A_TEAPOT = 418,
    INSUFFICIENT_SPACE_ON_RESOURCE = 419,
    METHOD_FAILURE = 420,
    MISDIRECTED_REQUEST = 421,
    UNPROCESSABLE_ENTITY = 422,
    LOCKED = 423,
    FAILED_DEPENDENCY = 424,
    UPGRADE_REQUIRED = 426,
    PRECONDITION_REQUIRED = 428,
    TOO_MANY_REQUESTS = 429,
    REQUEST_HEADER_FIELDS_TOO_LARGE = 431,
    UNAVAILABLE_FOR_LEGAL_REASONS = 451,
    INTERNAL_SERVER_ERROR = 500,
    NOT_IMPLEMENTED = 501,
    BAD_GATEWAY = 502,
    SERVICE_UNAVAILABLE = 503,
    GATEWAY_TIMEOUT = 504,
    HTTP_VERSION_NOT_SUPPORTED = 505,
    INSUFFICIENT_STORAGE = 507,
    NETWORK_AUTHENTICATION_REQUIRED = 511
}
export declare class HttpError extends Error {
    readonly statusCode: number;
    readonly headers: [string, string][] | undefined;
    constructor(statusCode: number, message?: string, headers?: [string, string][]);
    toString(): string;
    toResponse(): ResponseType;
}
export type StringRequestType = {
    uri: string;
    params: PathParamsType;
    headers: HeaderMapType;
    user?: UserType;
    body?: string;
};
export type StringResponseType = {
    headers?: [string, string][];
    status?: number;
    body: string;
};
export declare function stringHandler(f: (req: StringRequestType) => MaybeResponse<StringResponseType | string>): CallbackType;
export type HtmlResponseType = {
    headers?: [string, string][];
    status?: number;
    body: string;
};
export declare function htmlHandler(f: (req: StringRequestType) => MaybeResponse<HtmlResponseType | string>): CallbackType;
export type JsonRequestType = {
    uri: string;
    params: PathParamsType;
    headers: HeaderMapType;
    user?: UserType;
    body?: object | string;
};
export interface JsonResponseType {
    headers?: [string, string][];
    status?: number;
    body: object;
}
export declare function jsonHandler(f: (req: JsonRequestType) => MaybeResponse<JsonRequestType | object>): CallbackType;
export declare function addRoute(method: Method, route: string, callback: CallbackType): void;
export declare function dispatch(method: Method, route: string, uri: string, pathParams: [string, string][], headers: [string, string][], user: UserType | undefined, body: Uint8Array): Promise<ResponseType>;
export declare function addPeriodicCallback(milliseconds: number, cb: (cancel: () => void) => void): () => void;
export declare function query(queryStr: string, params: unknown[]): Promise<unknown[][]>;
export declare function execute(queryStr: string, params: unknown[]): Promise<number>;
export type ParsedPath = {
    path: string;
    query: URLSearchParams;
};
export declare function parsePath(path: string): ParsedPath;
export declare function decodeFallback(bytes: Uint8Array): string;
export declare function encodeFallback(string: string): Uint8Array;
