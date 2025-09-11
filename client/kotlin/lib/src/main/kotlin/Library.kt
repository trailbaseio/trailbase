package io.trailbase.client

import io.ktor.client.*
import io.ktor.client.call.body
import io.ktor.client.engine.cio.*
import io.ktor.client.plugins.contentnegotiation.*
import io.ktor.client.request.*
import io.ktor.client.statement.*
import io.ktor.http.*
import io.ktor.serialization.kotlinx.json.*
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.*

@Serializable data class User(val id: String, val email: String)

@Serializable
data class Tokens(val auth_token: String, val refresh_token: String, val csrf_token: String?)

class RecordApi(private val name: String, private val client: Client) {}

enum class Method {
  get,
  post,
  patch,
  delete,
}

class JwtToken {}

private class TokenState(val state: Pair<Tokens, JwtToken>?, val headers: Map<String, String>) {}

class Client(private val site: String, private val http: HttpClient = initClient()) {

  suspend fun login(email: String, password: String): Tokens {
    @Serializable data class Credentials(val email: String, val password: String)

    val response: HttpResponse =
            http.post("${site}/${AUTH_API}/login") {
              contentType(ContentType.Application.Json)
              setBody(Credentials(email, password))
            }

    return response.body()
  }

  suspend fun fetch(
          path: String,
          method: Method = Method.get,
          body: Any?,
          params: Map<String, List<String>> = emptyMap(),
  ): HttpResponse {
    val builder = URLBuilder("${site}/${path}")
    for ((k, v) in params) {
      builder.parameters.appendAll(k, v)
    }

    var headers = HeadersBuilder()
    headers.append("foo", "bar")
    // TODO: add headers.

    return when (method) {
      Method.get -> http.get(builder.build()) { headers = headers }
      Method.post ->
              http.post(builder.build()) {
                headers = headers
                setBody(body)
              }
      Method.patch ->
              http.patch(builder.build()) {
                headers = headers
                setBody(body)
              }
      Method.delete -> http.delete(builder.build()) { headers = headers }
    }
  }
}

private fun initClient(): HttpClient {
  val json = Json {
    ignoreUnknownKeys = true
    isLenient = true
    // prettyPrint = true // Ktor logging plugin will handle pretty printing if body logging is
    // enabled
  }

  val client =
          HttpClient(CIO.create()) {
            install(ContentNegotiation) {
              json(json) // Register Kotlinx.serialization converter
            }
          }

  return client
}

private const val AUTH_API: String = "api/auth/v1"
private const val RECORD_API: String = "api/records/v1"
