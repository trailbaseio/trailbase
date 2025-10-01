package io.trailbase.client

import io.ktor.client.*
import io.ktor.client.engine.cio.*
import io.ktor.client.plugins.contentnegotiation.*
import io.ktor.client.request.*
import io.ktor.client.statement.*
import kotlin.test.*
import kotlin.test.Test
import kotlin.time.Clock
import kotlinx.coroutines.*
import kotlinx.coroutines.test.*
import kotlinx.serialization.Serializable

@Serializable data class SimpleStrict(val id: String, val text_not_null: String)

@Serializable data class SimpleStrictInsert(val text_not_null: String)

class ClientTest {
  @Test
  fun foo() {
    assertTrue(true, "someLibraryMethod should return 'true'")
  }

  // WARN: TrailBase binding to localhost:4000 doesn't work. ktor only finds it when bound to
  // 127.0.0.1 or 0.0.0.0, no IPv6?.
  @Test
  fun `client authentication`() = runTest {
    val client = Client("http://localhost:4000")
    assertNull(client.user())
    assertNull(client.tokens())

    val tokens = client.login("admin@localhost", "secret")
    assertNotNull(tokens)
    assertEquals("admin@localhost", client.user()?.email)

    client.logout()
    assertNull(client.tokens())
  }

  suspend fun connect(): Client {
    val client = Client("http://localhost:4000")
    client.login("admin@localhost", "secret")
    return client
  }

  @OptIn(kotlin.time.ExperimentalTime::class)
  @Test
  fun `client records`() = runTest {
    val client = connect()
    val api = client.records("simple_strict_table")

    val now = Clock.System.now().toEpochMilliseconds() / 1000
    val messages = listOf("kotlin client test 0: =?&${now}", "kotlin client test 1: =?&${now}")

    val ids: MutableList<RecordId> = mutableListOf()
    for (msg in messages) {
      ids.add(api.create(SimpleStrictInsert(msg)))
    }

    val record0: SimpleStrict = api.read(ids[0])
    assertEquals(record0.id.toString(), ids[0].toString())
  }
}
