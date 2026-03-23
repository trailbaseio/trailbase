import EventSource
import Foundation
import FoundationNetworking

public protocol Transport {
  func fetch(
    path: String,
    headers: [(String, String)],
    method: String,
    queryParams: [URLQueryItem]?,
    body: Data?,
    throwOnError: Bool,
  ) async throws -> (HTTPURLResponse, Data)

  func stream(
    path: String,
    headers: [(String, String)],
    method: String,
    queryParams: [URLQueryItem]?,
  ) throws -> AsyncStream<EventSource.EventType>
}

extension Transport {
  // Just adds default parameters to protocol.
  internal func fetch(
    path: String,
    headers: [(String, String)],
    method: String,
    queryParams: [URLQueryItem]? = nil,
    body: Data? = nil,
    throwOnError: Bool = true,
  ) async throws -> (HTTPURLResponse, Data) {
    return try await fetch(
      path: path, headers: headers, method: method, queryParams: queryParams, body: body,
      throwOnError: throwOnError)
  }

  internal func stream(
    path: String,
    headers: [(String, String)],
    method: String,
    queryParams: [URLQueryItem]? = nil,
  ) throws -> AsyncStream<EventSource.EventType> {
    return try stream(path: path, headers: headers, method: method, queryParams: queryParams)
  }
}

public class DefaultTransport: Transport {
  private let base: URL
  private let session: URLSession

  init(base: URL) {
    self.base = base
    self.session = URLSession(configuration: URLSessionConfiguration.default)
  }

  public func fetch(
    path: String,
    headers: [(String, String)],
    method: String,
    queryParams: [URLQueryItem]? = nil,
    body: Data? = nil,
    throwOnError: Bool = true,
  ) async throws -> (HTTPURLResponse, Data) {
    assert(path.starts(with: "/"))
    guard var url = URL(string: path, relativeTo: self.base) else {
      throw ClientError.invalidUrl
    }
    if let params = queryParams {
      url.append(queryItems: params)
    }

    var request = URLRequest(url: url)
    for (name, value) in headers {
      request.setValue(value, forHTTPHeaderField: name)
    }
    request.httpMethod = method
    request.httpBody = body

    let (data, response) = try await self.session.data(for: request)
    guard let httpResponse = response as? HTTPURLResponse else {
      throw ClientError.invalidStatusCode(code: -1)
    }

    guard (200...299).contains(httpResponse.statusCode) || !throwOnError else {
      throw ClientError.invalidStatusCode(
        code: httpResponse.statusCode, body: String(data: data, encoding: .utf8))
    }

    return (httpResponse, data)
  }

  public func stream(
    path: String,
    headers: [(String, String)],
    method: String,
    queryParams: [URLQueryItem]? = nil,
  ) throws -> AsyncStream<EventSource.EventType> {
    guard var url = URL(string: path, relativeTo: self.base) else {
      throw ClientError.invalidUrl
    }
    if let params = queryParams {
      url.append(queryItems: params)
    }

    var request = URLRequest(url: url)
    for (name, value) in headers {
      request.setValue(value, forHTTPHeaderField: name)
    }
    request.httpMethod = method

    let source = EventSource(mode: .dataOnly)
    let response = source.dataTask(for: request)

    // TODO: We may want to return a Future<AsyncStream<_>> by awaiting `response.readyState == open`.
    return response.events()
  }
}
