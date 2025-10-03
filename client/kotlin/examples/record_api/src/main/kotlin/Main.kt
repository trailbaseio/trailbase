import io.trailbase.client.Client
import io.trailbase.examples.recordApi.SimpleStrictUpdate
import io.trailbase.examples.recordApi.create
import io.trailbase.examples.recordApi.delete
import io.trailbase.examples.recordApi.list
import io.trailbase.examples.recordApi.read
import io.trailbase.examples.recordApi.update
import kotlinx.coroutines.*

fun main() {
    runBlocking {
        val client = Client("http://localhost:4000")
        client.login("admin@localhost", "secret")

        val id = create(client)

        println("Id: ${id}")

        val record0 = read(client, id)
        assert(record0.text_not_null == "test")

        update(client, id, SimpleStrictUpdate(text_not_null = "updated"))
        val record1 = read(client, id)
        assert(record0.text_not_null == "updated")

        delete(client, id)

        // Separate test listing of movies
        val response = list(client)
        assert(response.records.size == 3)
    }
}
