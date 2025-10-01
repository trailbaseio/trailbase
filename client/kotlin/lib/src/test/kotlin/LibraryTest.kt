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
import org.junit.jupiter.api.assertThrows

@Serializable data class SimpleStrict(val id: String, val text_not_null: String)

@Serializable data class SimpleStrictInsert(val text_not_null: String)

@Serializable data class SimpleStrictUpdate(val text_not_null: String?)

class ClientTest {
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
        assertEquals(RecordId.string(record0.id), ids[0])

        if (true) {
            val response: ListResponse<SimpleStrict> =
                api.list(
                    filters =
                        listOf<FilterBase>(Filter(column = "text_not_null", value = messages[0]))
                )

            assertEquals(messages[0], response.records[0].text_not_null)
        }

        if (true) {
            val response: ListResponse<SimpleStrict> =
                api.list(
                    order = listOf("+text_not_null"),
                    filters =
                        listOf<FilterBase>(
                            Filter(column = "text_not_null", value = "% =?&${now}", CompareOp.like)
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
                            Filter(column = "text_not_null", value = "% =?&${now}", CompareOp.like)
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
                            Filter(column = "text_not_null", value = "% =?&${now}", CompareOp.like)
                        )
                )

            assertEquals(response.total_count, 2)
            assertEquals(
                messages.reversed().subList(0, 1),
                response.records.map { it.text_not_null }
            )
        }

        val updateMessage = "kotlin client update test 0: =?&${now}"
        api.update(ids[0], SimpleStrictUpdate(text_not_null = updateMessage))
        val updatedRecord: SimpleStrict = api.read(ids[0])
        assertEquals(updateMessage, updatedRecord.text_not_null)

        api.delete(ids[0])
        assertThrows<HttpException>({ api.read<SimpleStrict>(ids[0]) })
    }
}
