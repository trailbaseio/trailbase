package io.trailbase.client

import io.ktor.client.*
import io.ktor.client.engine.cio.*
import io.ktor.client.plugins.contentnegotiation.*
import io.ktor.client.request.*
import io.ktor.client.statement.*
import kotlin.test.Test
import kotlin.test.assertTrue
import kotlinx.coroutines.*
import kotlinx.coroutines.test.*

class ClientTest {
  @Test
  fun foo() {
    assertTrue(true, "someLibraryMethod should return 'true'")
  }

  // WARN: Trailbase binding to localhost:4000 doesn't work. ktor only finds it when bound to
  // 127.0.0.1 or 0.0.0.0, no IPv6?.
  @Test
  fun `client test`() = runTest {
    val client = Client("http://localhost:4000")
    val tokens = client.login("admin@localhost", "secret")
    println("Tokens: ${tokens}")

    assertTrue(false, "what the heck")
  }
}
