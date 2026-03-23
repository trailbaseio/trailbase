import EventSource
import Foundation
import FoundationNetworking
import Synchronization

public struct User: Hashable, Equatable {
  let sub: String
  let email: String
}

// NOTE: Making this explicitly public breaks compiler.
public struct Tokens: Codable, Hashable, Equatable, Sendable {
  let auth_token: String
  let refresh_token: String?
  let csrf_token: String?
}

public struct MultiFactorAuthToken: Codable, Hashable, Equatable, Sendable {
  let mfa_token: String
}

public struct Pagination {
  public var cursor: String? = nil
  public var limit: UInt? = nil
  public var offset: UInt? = nil

  public init(cursor: String? = nil, limit: UInt? = nil, offset: UInt? = nil) {
    self.cursor = cursor
    self.limit = limit
    self.offset = offset
  }
}

private struct JwtTokenClaims: Decodable, Hashable {
  let sub: String
  let iat: Int64
  let exp: Int64
  let email: String
  let csrf_token: String
}

private struct TokenState {
  let state: (Tokens, JwtTokenClaims)?
  let headers: [(String, String)]

  init(tokens: Tokens?) throws {
    if let t = tokens {
      guard let claims = decodeJwtTokenClaims(t.auth_token) else {
        throw ClientError.invalidJwt
      }

      self.state = (t, claims)
      self.headers = buildHeaders(tokens: tokens)
      return
    }

    self.state = nil
    self.headers = buildHeaders(tokens: tokens)
  }
}

public enum RecordId: CustomStringConvertible {
  case string(String)
  case int(Int64)

  public var description: String {
    return switch self {
    case .string(let id): id
    case .int(let id): id.description
    }
  }
}

private struct RecordIdResponse: Codable {
  public let ids: [String]
}

public struct ListResponse<T: Decodable>: Decodable {
  public let cursor: String?
  public let total_count: Int64?
  public let records: [T]
}

public enum CompareOp {
  case Equal
  case NotEqual
  case LessThan
  case LessThanEqual
  case GreaterThan
  case GreaterThanEqual
  case Like
  case Regexp
  case StWithin
  case StIntersects
  case StContains
}

extension CompareOp {
  func op() -> String {
    return switch self {
    case .Equal: "$eq"
    case .NotEqual: "$ne"
    case .LessThan: "$lt"
    case .LessThanEqual: "$lte"
    case .GreaterThan: "$gt"
    case .GreaterThanEqual: "$gte"
    case .Like: "$like"
    case .Regexp: "$re"
    case .StWithin: "@within"
    case .StIntersects: "@intersects"
    case .StContains: "@contains"
    }
  }
}

public enum Filter {
  case Filter(column: String, op: CompareOp? = nil, value: String)
  case And(filters: [Filter])
  case Or(filters: [Filter])
}

public class RecordApi {
  let client: Client
  let name: String

  public init(client: Client, name: String) {
    self.client = client
    self.name = name
  }

  public func list<T: Decodable>(
    pagination: Pagination? = nil,
    order: [String]? = nil,
    filters: [Filter]? = nil,
    expand: [String]? = nil,
    count: Bool = false,
  ) async throws -> ListResponse<T> {
    var queryParams: [URLQueryItem] = []

    if let p = pagination {
      if let cursor = p.cursor {
        queryParams.append(URLQueryItem(name: "cursor", value: cursor))
      }
      if let limit = p.limit {
        queryParams.append(URLQueryItem(name: "limit", value: "\(limit)"))
      }
      if let offset = p.offset {
        queryParams.append(URLQueryItem(name: "offset", value: "\(offset)"))
      }
    }

    if let o = order {
      if !o.isEmpty {
        queryParams.append(URLQueryItem(name: "order", value: o.joined(separator: ",")))
      }
    }

    if let e = expand {
      if !e.isEmpty {
        queryParams.append(URLQueryItem(name: "expand", value: e.joined(separator: ",")))
      }
    }

    if count {
      queryParams.append(URLQueryItem(name: "count", value: "true"))
    }

    func traverseFilters(path: String, filter: Filter) {
      switch filter {
      case .Filter(let column, let op, let value):
        if op != nil {
          queryParams.append(
            URLQueryItem(name: "\(path)[\(column)][\(op!.op())]", value: value))
        } else {
          queryParams.append(
            URLQueryItem(name: "\(path)[\(column)]", value: value))
        }
        break
      case .And(let filters):
        for (i, filter) in filters.enumerated() {
          traverseFilters(path: "\(path)[$and][\(i)]", filter: filter)
        }
        break
      case .Or(let filters):
        for (i, filter) in filters.enumerated() {
          traverseFilters(path: "\(path)[$or][\(i)]", filter: filter)
        }
        break
      }
    }

    if let f = filters {
      for filter in f {
        traverseFilters(path: "filter", filter: filter)
      }
    }

    let (_, data) = try await self.client.fetch(
      path: "/\(RECORD_API)/\(name)",
      method: "GET",
      queryParams: queryParams,
      body: nil,
    )

    return try JSONDecoder().decode(ListResponse.self, from: data)
  }

  public func read<T: Decodable>(recordId: RecordId, expand: [String]? = nil) async throws -> T {
    let queryParams: [URLQueryItem]? =
      if let e = expand {
        [URLQueryItem(name: "expand", value: e.joined(separator: ","))]
      } else {
        nil
      }

    let (_, data) = try await self.client.fetch(
      path: "/\(RECORD_API)/\(name)/\(recordId)", method: "GET", queryParams: queryParams)

    return try JSONDecoder().decode(T.self, from: data)
  }

  // TODO: Implement bulk creation.
  public func create<T: Encodable>(record: T) async throws -> RecordId {
    let body = try JSONEncoder().encode(record)
    let (_, data) = try await self.client.fetch(
      path: "/\(RECORD_API)/\(name)", method: "POST", body: body)

    let response = try JSONDecoder().decode(RecordIdResponse.self, from: data)
    if response.ids.count != 1 {
      throw ClientError.invalidResponse("expected one id")
    }
    return RecordId.string(response.ids[0])
  }

  public func update<T: Encodable>(recordId: RecordId, record: T) async throws {
    let body = try JSONEncoder().encode(record)
    let _ = try await self.client.fetch(
      path: "/\(RECORD_API)/\(name)/\(recordId)", method: "PATCH", body: body)
  }

  public func delete(recordId: RecordId) async throws {
    let _ = try await self.client.fetch(
      path: "/\(RECORD_API)/\(name)/\(recordId)", method: "DELETE")
  }

  // public func subscribe(recordId: RecordId) async throws -> AsyncStream<Event> {
  //   let events = try await self.client.stream(
  //     path: "/\(RECORD_API)/\(name)/subscribe/\(recordId)", method: "GET")
  //
  //   return events.filterMap({
  //     (event) -> Event? in
  //
  //     print("subscribe(): GOT", event)
  //
  //     switch event {
  //     case .event(let event):
  //       if let data = event.data?.data(using: .utf8) {
  //         return try? JSONDecoder().decode(Event.self, from: Data(data))
  //       }
  //     default:
  //       break
  //     }
  //     return nil
  //   })
  // }
  //
  // public func subscribeAll() async throws -> AsyncStream<Event> {
  //   return try await subscribe(recordId: RecordId.string("*"))
  // }
}

public enum ClientError: Error {
  case invalidUrl
  case invalidStatusCode(code: Int, body: String? = nil)
  case invalidResponse(String?)
  case invalidJwt
  case unauthenticated
  case invalidFilter(String)
  case invalidEvent
}

public class Client {
  private let base: URL
  private let transport: Transport
  private let tokenState: Mutex<TokenState>

  public init(site: URL, tokens: Tokens? = nil, transport: Transport? = nil) throws {
    self.base = site
    self.transport = transport ?? DefaultTransport(base: site)
    self.tokenState = Mutex(try TokenState(tokens: tokens))
  }

  public var site: URL {
    return self.base
  }

  public var tokens: Tokens? {
    return self.tokenState.withLock({ (state) in
      if let tokens = state.state?.0 {
        return tokens
      }
      return nil
    })
  }

  public var user: User? {
    return self.tokenState.withLock({ (state) in
      if let claims = state.state?.1 {
        return User(sub: claims.sub, email: claims.email)
      }
      return nil
    })
  }

  public func records(_ name: String) -> RecordApi {
    return RecordApi(client: self, name: name)
  }

  public func refresh() async throws {
    guard let (headers, refreshToken) = getHeaderAndRefreshToken() else {
      throw ClientError.unauthenticated
    }

    let newTokens = try await Client.doRefreshToken(
      client: self.transport, headers: headers, refreshToken: refreshToken)

    self.tokenState.withLock({ (tokens) in
      tokens = newTokens
    })
  }

  public func login(email: String, password: String) async throws -> MultiFactorAuthToken? {
    struct Credentials: Codable {
      let email: String
      let password: String
    }

    let body = try JSONEncoder().encode(Credentials(email: email, password: password))
    let (httpResponse, data) = try await self.fetch(
      path: "/\(AUTH_API)/login", method: "POST", body: body, throwOnError: false)

    if httpResponse.statusCode == 403 {
      return try JSONDecoder().decode(MultiFactorAuthToken.self, from: data)
    } else if httpResponse.statusCode != 200 {
      throw ClientError.invalidStatusCode(
        code: httpResponse.statusCode, body: String(data: data, encoding: .utf8))
    }

    let tokens = try JSONDecoder().decode(Tokens.self, from: data)
    let _ = try updateTokens(tokens: tokens)

    return nil
  }

  public func loginSecond(mfaToken: MultiFactorAuthToken, totpCode: String) async throws {
    struct Credentials: Codable {
      let mfa_token: String
      let totp: String
    }

    let body = try JSONEncoder().encode(
      Credentials(mfa_token: mfaToken.mfa_token, totp: totpCode))
    let (_, data) = try await self.fetch(
      path: "/\(AUTH_API)/login_mfa", method: "POST", body: body)

    let tokens = try JSONDecoder().decode(Tokens.self, from: data)
    let _ = try updateTokens(tokens: tokens)
  }

  public func requestOtp(email: String, redirectUri: String? = nil) async throws {
    struct Credentials: Codable {
      let email: String
      let redirect_uri: String?
    }

    let body = try JSONEncoder().encode(
      Credentials(email: email, redirect_uri: redirectUri))
    let (_, _) = try await self.fetch(
      path: "/\(AUTH_API)/otp/request", method: "POST", body: body)
  }

  public func loginOtp(email: String, code: String) async throws {
    struct Credentials: Codable {
      let email: String
      let code: String
    }

    let body = try JSONEncoder().encode(Credentials(email: email, code: code))
    let (_, _) = try await self.fetch(
      path: "/\(AUTH_API)/otp/login", method: "POST", body: body)
  }

  public func logout() async throws {
    struct LogoutRequest: Encodable {
      let refresh_token: String
    }

    if let (_, refreshToken) = getHeaderAndRefreshToken() {
      let body = try JSONEncoder().encode(LogoutRequest(refresh_token: refreshToken))
      let _ = try await self.fetch(
        path: "/\(AUTH_API)/logout", method: "POST", body: body)
    } else {
      let _ = try await self.fetch(
        path: "/\(AUTH_API)/logout", method: "GET")
    }

    let _ = try self.updateTokens(tokens: nil)
  }

  private func updateTokens(tokens: Tokens?) throws -> TokenState {
    let state = try TokenState(tokens: tokens)
    self.tokenState.withLock({ (tokens) in
      tokens = state
    })
    return state
  }

  fileprivate func fetch(
    path: String,
    method: String,
    queryParams: [URLQueryItem]? = nil,
    body: Data? = nil,
    throwOnError: Bool = true,
  ) async throws -> (HTTPURLResponse, Data) {
    var (headers, refreshToken) = getHeadersAndRefreshTokenIfExpired()
    if let rt = refreshToken {
      let newTokens = try await Client.doRefreshToken(
        client: self.transport, headers: headers, refreshToken: rt)
      headers = newTokens.headers
      self.tokenState.withLock({ (tokens) in
        tokens = newTokens
      })
    }

    return try await transport.fetch(
      path: path, headers: headers, method: method, queryParams: queryParams, body: body,
      throwOnError: throwOnError)
  }

  fileprivate func stream(
    path: String,
    method: String,
    queryParams: [URLQueryItem]? = nil,
  ) async throws -> AsyncStream<EventSource.EventType> {
    var (headers, refreshToken) = getHeadersAndRefreshTokenIfExpired()
    if let rt = refreshToken {
      let newTokens = try await Client.doRefreshToken(
        client: self.transport, headers: headers, refreshToken: rt)
      headers = newTokens.headers
      self.tokenState.withLock({ (tokens) in
        tokens = newTokens
      })
    }

    return try transport.stream(
      path: path, headers: headers, method: method, queryParams: queryParams)
  }

  private func getHeaderAndRefreshToken() -> ([(String, String)], String)? {
    return self.tokenState.withLock({ (tokens) in
      if let s = tokens.state {
        if let refreshToken = s.0.refresh_token {
          return (tokens.headers, refreshToken)
        }
      }
      return nil
    })
  }

  private func getHeadersAndRefreshTokenIfExpired() -> ([(String, String)], String?) {
    func shouldRefresh(exp: Int64) -> Bool {
      Double(exp) - 60 < NSDate().timeIntervalSince1970
    }

    return self.tokenState.withLock({ (tokens) in
      if let s = tokens.state {
        if shouldRefresh(exp: s.1.exp) {
          return (tokens.headers, s.0.refresh_token)
        }
      }
      return (tokens.headers, nil)
    })
  }

  private static func doRefreshToken(
    client: Transport, headers: [(String, String)], refreshToken: String
  ) async throws -> TokenState {
    struct RefreshRequest: Encodable {
      let refresh_token: String
    }
    let body = try JSONEncoder().encode(RefreshRequest(refresh_token: refreshToken))
    let (_, data) = try await client.fetch(
      path: "/\(AUTH_API)/refresh", headers: headers, method: "POST", body: body)

    struct RefreshResponse: Decodable {
      let auth_token: String
      let csrf_token: String?
    }

    let refreshResponse = try JSONDecoder().decode(RefreshResponse.self, from: data)
    let tokens = Tokens(
      auth_token: refreshResponse.auth_token,
      refresh_token: refreshToken,
      csrf_token: refreshResponse.csrf_token,
    )
    return try TokenState(tokens: tokens)
  }
}

private func buildHeaders(tokens: Tokens?) -> [(String, String)] {
  var headers: [(String, String)] = [
    ("Content-Type", "application/json")
  ]

  if let t = tokens {
    headers.append(("Authorization", "Bearer \(t.auth_token)"))

    if let rt = t.refresh_token {
      headers.append(("Refresh-Token", rt))
    }
    if let csrf = t.csrf_token {
      headers.append(("CSRF-Token", csrf))
    }
  }

  return headers
}

private func base64URLDecode(_ value: String) -> Data? {
  var base64 = value.replacingOccurrences(of: "-", with: "+")
    .replacingOccurrences(of: "_", with: "/")
  let length = Double(base64.lengthOfBytes(using: .utf8))
  let requiredLength = 4 * ceil(length / 4.0)
  let paddingLength = requiredLength - length
  if paddingLength > 0 {
    let padding = "".padding(toLength: Int(paddingLength), withPad: "=", startingAt: 0)
    base64 = base64 + padding
  }
  return Data(base64Encoded: base64, options: .ignoreUnknownCharacters)
}

private func decodeJwtTokenClaims(_ jwt: String) -> JwtTokenClaims? {
  let parts = jwt.split(separator: ".")
  guard parts.count == 3 else {
    return nil
  }

  let payload = String(parts[1])
  guard let data = base64URLDecode(payload) else {
    return nil
  }

  do {
    let claims = try JSONDecoder().decode(JwtTokenClaims.self, from: data)
    return claims
  } catch {
    return nil
  }
}

// Used for EventSource.events().map().
extension AsyncStream {
  public func map<Transformed: Sendable>(
    _ transform: @escaping @Sendable (Element) -> Transformed
  ) -> AsyncStream<Transformed> where Element: Sendable {
    return AsyncStream<Transformed> { continuation in
      let task = Task {
        for await element in self {
          continuation.yield(transform(element))
        }
        continuation.finish()
      }
      continuation.onTermination = { _ in
        task.cancel()
      }
    }
  }

  public func filterMap<Transformed: Sendable>(
    _ transform: @escaping @Sendable (Element) -> Transformed?
  ) -> AsyncStream<Transformed> where Element: Sendable {
    return AsyncStream<Transformed> { continuation in
      let task = Task {
        for await element in self {
          let transformed = transform(element)
          if transformed != nil {
            continuation.yield(transformed!)
          }
        }
        continuation.finish()
      }
      continuation.onTermination = { _ in
        task.cancel()
      }
    }
  }
}

private let AUTH_API = "api/auth/v1"
private let RECORD_API = "api/records/v1"
