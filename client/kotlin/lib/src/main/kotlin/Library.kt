package io.trailbase.client

import io.ktor.client.*
import io.ktor.client.call.body
import io.ktor.client.engine.cio.*
import io.ktor.client.plugins.contentnegotiation.*
import io.ktor.client.request.*
import io.ktor.client.statement.*
import io.ktor.http.*
import io.ktor.serialization.kotlinx.json.*
import kotlin.io.encoding.Base64
import kotlin.time.Clock
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.*

@Serializable data class User(val id: String, val email: String)

@Serializable
data class Tokens(val auth_token: String, val refresh_token: String?, val csrf_token: String?)

@Serializable
data class JwtTokenClaims(
        val sub: String,
        val iat: Long,
        val exp: Long,
        val email: String,
        val csrf_token: String
)

class TokenState(val state: Pair<Tokens, JwtTokenClaims>?, val headers: Map<String, List<String>>) {
  companion object {
    fun build(tokens: Tokens?): TokenState {
      return TokenState(
              if (tokens != null) Pair(tokens, decodeJwtTokenClaims(tokens.auth_token)) else null,
              buildHeaders(tokens)
      )
    }
  }

  fun user(): User? {
    val jwt = state?.second
    return if (jwt != null) User(jwt.sub, jwt.email) else null
  }

  @OptIn(kotlin.time.ExperimentalTime::class)
  internal fun shouldRefresh(): String? {
    if (state != null) {
      val now = Clock.System.now().toEpochMilliseconds() / 1000
      if (state.second.exp - 60 < now) {
        return state.first.refresh_token
      }
    }
    return null
  }
}

abstract class RecordId {
  override abstract fun toString(): String
}

class StringRecordId(private val id: String) : RecordId() {
  override fun toString(): String {
    return id
  }
}

@Serializable private data class ResponseRecordIds(val ids: List<String>)

class RecordApi(val name: String, val client: Client) {
  suspend fun <T> create(record: T): RecordId {
    val response = client.fetch("${RECORD_API}/${name}", Method.post, record)
    val ids: ResponseRecordIds = response.body()
    return StringRecordId(ids.ids[0])
  }

  suspend inline fun <reified T> read(id: RecordId, expand: List<String>? = null): T {
    if (expand != null) {
      return client.fetch(
                      "${RECORD_API}/${name}/${id}",
                      params = mapOf(Pair("expand", listOf(expand.joinToString(","))))
              )
              .body()
    }
    return client.fetch("${RECORD_API}/${name}/${id}").body()
  }
}

enum class Method {
  get,
  post,
  patch,
  delete,
}

class Client(
        private val site: Url,
        private var tokenState: TokenState,
        private val http: HttpClient = initClient()
) {
  constructor(
          site: String
  ) : this(
          Url(site),
          TokenState.build(null),
  )

  fun site(): Url {
    return this.site
  }

  fun tokens(): Tokens? {
    return tokenState.state?.first
  }

  fun user(): User? {
    return tokenState.user()
  }

  fun records(name: String): RecordApi {
    return RecordApi(name, this)
  }

  suspend fun login(email: String, password: String): Tokens {
    @Serializable data class Credentials(val email: String, val password: String)

    val tokens: Tokens =
            fetch("${AUTH_API}/login", Method.post, Credentials(email, password)).body()
    tokenState = TokenState.build(tokens)
    return tokens
  }

  suspend fun logout() {
    try {
      val refreshToken = tokenState.state?.first?.refresh_token
      if (refreshToken != null) {
        @Serializable data class Body(val refresh_token: String)

        fetch("${AUTH_API}/logout", Method.post, Body(refreshToken))
      } else {
        fetch("${AUTH_API}/logout")
      }
    } finally {
      tokenState = TokenState.build(null)
    }
  }

  suspend fun refreshAuthToken() {
    val refreshToken = tokenState.shouldRefresh()
    if (refreshToken != null) {
      tokenState = refreshTokensImpl(refreshToken)
    }
  }

  suspend fun fetch(
          path: String,
          method: Method = Method.get,
          body: Any? = null,
          params: Map<String, List<String>> = emptyMap(),
  ): HttpResponse {
    val refreshToken = tokenState.shouldRefresh()
    if (refreshToken != null) {
      tokenState = refreshTokensImpl(refreshToken)
    }

    return http.request(site) {
      this.method =
              when (method) {
                Method.get -> HttpMethod.Get
                Method.post -> HttpMethod.Post
                Method.patch -> HttpMethod.Patch
                Method.delete -> HttpMethod.Delete
              }
      url {
        path(path)
        parametersOf(params)
      }
      headers { tokenState.headers.forEach { appendAll(it.key, it.value) } }
      contentType(ContentType.Application.Json)
      setBody(body)
    }
  }

  private suspend fun refreshTokensImpl(refreshToken: String): TokenState {
    @Serializable data class Body(val refresh_token: String)

    val tokens: Tokens =
            http
                    .post(site) {
                      url { path("${AUTH_API}/refresh") }
                      contentType(ContentType.Application.Json)
                      headers { tokenState.headers.forEach { appendAll(it.key, it.value) } }
                      setBody(Body(refreshToken))
                    }
                    .body()

    return TokenState.build(tokens)
  }
}

private fun initClient(): HttpClient {
  return HttpClient(CIO.create()) {
    install(ContentNegotiation) {
      // Register Kotlinx.serialization converter
      json(
              Json {
                ignoreUnknownKeys = true
                isLenient = true
              }
      )
    }
  }
}

private fun buildHeaders(tokens: Tokens?): Map<String, List<String>> {
  val headers: MutableMap<String, List<String>> = mutableMapOf()

  if (tokens != null) {
    headers["Authorization"] = listOf("Bearer ${tokens.auth_token}")

    val refresh = tokens.refresh_token
    if (refresh != null) {
      headers["Refresh-Token"] = listOf(refresh)
    }

    val csrf = tokens.csrf_token
    if (csrf != null) {
      headers["CSRF-Token"] = listOf(csrf)
    }
  }

  return headers
}

@OptIn(kotlin.io.encoding.ExperimentalEncodingApi::class)
private fun decodeJwtTokenClaims(jwt: String): JwtTokenClaims {
  val parts = jwt.split('.')
  if (parts.size != 3) {
    throw Exception("Invalid JWT format")
  }

  val decoded =
          Base64.UrlSafe.withPadding(Base64.PaddingOption.PRESENT_OPTIONAL)
                  .decode(parts[1])
                  .decodeToString()

  return Json.decodeFromString(decoded)
}

private const val AUTH_API: String = "api/auth/v1"
const val RECORD_API: String = "api/records/v1"
