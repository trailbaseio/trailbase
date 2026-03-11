package io.trailbase.client

import dev.samstevens.totp.code.*
import dev.samstevens.totp.time.SystemTimeProvider
import io.ktor.client.*
import io.ktor.client.engine.cio.*
import io.ktor.client.plugins.contentnegotiation.*
import io.ktor.client.request.*
import io.ktor.client.statement.*
import io.ktor.http.Url
import io.ktor.http.isSuccess
import java.lang.Process
import java.lang.ProcessBuilder
import java.nio.file.Path
import java.nio.file.Paths
import kotlin.io.path.name
import kotlin.test.*
import kotlin.time.Clock
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.test.*
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.*
import org.junit.jupiter.api.AfterAll
import org.junit.jupiter.api.BeforeAll
import org.junit.jupiter.api.MethodOrderer
import org.junit.jupiter.api.Order
import org.junit.jupiter.api.TestInstance
import org.junit.jupiter.api.TestMethodOrder
import org.junit.jupiter.api.assertThrows

@Serializable data class SimpleStrict(val id: String, val text_not_null: String)

@Serializable data class SimpleStrictInsert(val text_not_null: String)

@Serializable data class SimpleStrictUpdate(val text_not_null: String?)

@TestInstance(TestInstance.Lifecycle.PER_CLASS)
@TestMethodOrder(MethodOrderer.OrderAnnotation::class)
// @TestClassOrder(ClassOrderer.OrderAnnotation.class)
class ClientTest {
  companion object {
    const val port = 4061
    const val address = "127.0.0.1:${port}"
    var process: Process? = null
  }

  @BeforeAll
  fun setUpAll() {
    val workingDirectory: Path = Paths.get("").toAbsolutePath().parent
    assertEquals("kotlin", workingDirectory.name)
    // Depot path relative to working directory.
    val depotPath = "../testfixture"

    val result =
            ProcessBuilder("cargo", "build")
                    .directory(workingDirectory.toFile())
                    .redirectOutput(ProcessBuilder.Redirect.INHERIT)
                    .redirectError(ProcessBuilder.Redirect.INHERIT)
                    .start()
                    .waitFor()
    if (result > 0) {
      throw Exception()
    }

    process =
            ProcessBuilder(
                            "cargo",
                            "run",
                            "--",
                            "--data-dir=${depotPath}",
                            "run",
                            "--address=${address}",
                            "--runtime-threads=2"
                    )
                    .directory(workingDirectory.toFile())
                    .redirectOutput(ProcessBuilder.Redirect.INHERIT)
                    .redirectError(ProcessBuilder.Redirect.INHERIT)
                    .start()

    var success = false
    runBlocking {
      val client = HttpClient(CIO.create())
      val url = Url("http://${address}/api/healthcheck")

      for (i in 0..100) {
        try {
          val response = client.get(url)
          if (response.status.isSuccess()) {
            success = true
            break
          }
        } catch (err: Throwable) {
          println("Trying to connect to TrailBase ${i+1}/50: ${err}")
        }

        // No point in waiting longer.
        if (process?.isAlive() == false) {
          break
        }
        delay(500)
      }
    }

    if (!success) {
      process?.destroyForcibly()
      throw Exception("Cargo run failed: ${process?.exitValue()}")
    }
  }

  @AfterAll
  fun tearDownAll() {
    process?.destroyForcibly()?.waitFor()
    process?.exitValue()
  }

  @Test
  fun `filter params`() {
    val params: MutableMap<String, String> = mutableMapOf()

    val filters =
            listOf(
                    Filter("col0", "0", CompareOp.greaterThan),
                    Filter("col0", "5", CompareOp.lessThan)
            )
    for (filter in filters) {
      addFiltersToParams(params, "filter", filter)
    }

    assertEquals(2, params.size)
  }

  // WARN: TrailBase binding to localhost:4000 doesn't work. ktor only finds it when bound to
  // 127.0.0.1 or 0.0.0.0, no IPv6?.
  @Test
  fun `client authentication`() = runTest {
    val client = Client("http://${address}")
    assertNull(client.user())
    assertNull(client.tokens())

    val mfaToken = client.login("admin@localhost", "secret")
    assertNull(mfaToken)
    assertNotNull(client.tokens())
    assertEquals("admin@localhost", client.user()?.email)

    client.logout()
    assertNull(client.tokens())
  }

  @Test
  fun `client OTP authentication`() = runTest {
    val client = Client("http://${address}")

    // NOTE: Since we don't have access to the sent emails, we just make sure the endpoint responds
    // ok.
    client.requestOtp("fake0@localhost")
    client.requestOtp("fake1@localhost", "/target")

    val exception0 =
            assertThrows<HttpException>({ client.loginOtp("fake1@localhost", "invalidCode") })
    assertEquals(exception0.status, 401)

    val exception1 =
            assertThrows<HttpException>({ client.loginOtp("unrequested@localhost", "invalidCode") })
    assertEquals(exception1.status, 401)
  }

  @Test
  fun `client multi-factor authentication`() = runTest {
    val client = Client("http://${address}")
    val mfaToken = client.login("alice@trailbase.io", "secret")
    assertNotNull(mfaToken)

    val secret = "YCUTAYEZ346ZUEI7FLCG57BOMZQHHRA5"

    val timeProvider = SystemTimeProvider()
    val currentBucket = Math.floorDiv(timeProvider.getTime(), 30)
    val g = DefaultCodeGenerator(HashingAlgorithm.SHA1)
    val code: String = g.generate(secret, currentBucket)
    assertEquals(6, code.length)

    client.login2nd(mfaToken!!, code)
    assertNotNull(client.user())
    assertEquals("alice@trailbase.io", client.user()?.email)

    client.logout()
    assertNull(client.tokens())
  }

  suspend fun connect(): Client {
    val client = Client("http://${address}")
    client.login("admin@localhost", "secret")
    return client
  }

  @OptIn(kotlin.time.ExperimentalTime::class)
  @Order(1)
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
    assertEquals(RecordId.string(record0.id), ids[0])

    if (true) {
      val response: ListResponse<SimpleStrict> =
              api.list(
                      filters =
                              listOf<FilterBase>(
                                      Filter(column = "text_not_null", value = messages[0])
                              )
              )

      assertEquals(messages[0], response.records[0].text_not_null)
    }

    if (true) {
      val response: ListResponse<SimpleStrict> =
              api.list(
                      order = listOf("+text_not_null"),
                      filters =
                              listOf<FilterBase>(
                                      Filter(
                                              column = "text_not_null",
                                              value = "% =?&${now}",
                                              CompareOp.like
                                      )
                              )
              )
      assertEquals(messages, response.records.map { it.text_not_null })
    }

    if (true) {
      val response: ListResponse<SimpleStrict> =
              api.list(
                      order = listOf("-text_not_null"),
                      filters =
                              listOf<FilterBase>(
                                      Filter(
                                              column = "text_not_null",
                                              value = "% =?&${now}",
                                              CompareOp.like
                                      )
                              )
              )
      assertEquals(messages.reversed(), response.records.map { it.text_not_null })
    }

    if (true) {
      val response: ListResponse<SimpleStrict> =
              api.list(
                      count = true,
                      pagination = Pagination(limit = 1),
                      order = listOf("-text_not_null"),
                      filters =
                              listOf<FilterBase>(
                                      Filter(
                                              column = "text_not_null",
                                              value = "% =?&${now}",
                                              CompareOp.like
                                      )
                              )
              )

      assertEquals(response.total_count, 2)
      assertEquals(messages.reversed().subList(0, 1), response.records.map { it.text_not_null })
    }

    val updateMessage = "kotlin client update test 0: =?&${now}"
    api.update(ids[0], SimpleStrictUpdate(text_not_null = updateMessage))
    val updatedRecord: SimpleStrict = api.read(ids[0])
    assertEquals(updateMessage, updatedRecord.text_not_null)

    api.delete(ids[0])
    assertThrows<HttpException>({ api.read<SimpleStrict>(ids[0]) })
  }

  @OptIn(kotlin.time.ExperimentalTime::class)
  @Order(100)
  @Test
  fun `client record subscriptions`() = runTest {
    val client = connect()
    val api = client.records("simple_strict_table")

    val flow = api.subscribe<SimpleStrict>(RecordId.string("*"))

    val now = Clock.System.now().toEpochMilliseconds() / 1000
    val id = api.create(SimpleStrictInsert("kotlin subscription test 0: =?&${now}"))
    api.delete(id)

    val result = mutableListOf<DbEvent>()
    flow.take(2).toList(result)

    assertEquals(2, result.count())

    val insert: SimpleStrict =
            localJsonSerializer.decodeFromJsonElement((result[0] as DbEvent.Insert).obj)
    assertEquals(insert.id, id.id())

    val delete: SimpleStrict =
            localJsonSerializer.decodeFromJsonElement((result[1] as DbEvent.Delete).obj)
    assertEquals(delete.id, id.id())
  }
}

val localJsonSerializer = Json {
  ignoreUnknownKeys = true
  isLenient = true
}
